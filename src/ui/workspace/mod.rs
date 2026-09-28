//! root view: owns tabs and handles new tab / close tab.

mod tab_bar;
mod tab_title;

use std::time::Duration;

use futures::{
    FutureExt, StreamExt,
    channel::mpsc::{UnboundedSender, unbounded},
};
use gpui::{
    Action, App, Context, Entity, EntityId, Focusable, ScrollHandle, Subscription, Task, Window,
    actions, prelude::*,
};

use tab_title::TitleInputs;

use crate::{
    settings::{Settings, TabTitleBlock},
    terminal::{Event, TerminalBuilder},
    theme::Theme,
    ui::terminal_view::TerminalView,
};

actions!(workspace, [NewTab, CloseTab, NextTab]);

/// activate tab at this 0 based index
#[derive(Clone, PartialEq, Action)]
#[action(namespace = workspace, no_json)]
pub struct ActivateTab(pub usize);

// programs, folders and command output change without events, so titles are polled
const TITLE_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

// tab title blocks, window title blocks, active tab, per tab inputs
type TitleSnapshot = (
    Vec<TabTitleBlock>,
    Vec<TabTitleBlock>,
    usize,
    Vec<(EntityId, TitleInputs)>,
);

struct Tab {
    view: Entity<TerminalView>,
    /// built from `tab_title` blocks, empty until the first refresh
    title: String,
    _subscription: Subscription,
}

pub struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    /// tab row scroll state, also records where each tab was laid out
    tab_scroll: ScrollHandle,
    /// built from `window_title` blocks for the active tab
    window_title: String,
    refresh_titles: UnboundedSender<()>,
    _title_task: Task<()>,
}

impl Workspace {
    /// create workspace with one terminal tab
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (refresh_titles, mut refresh_rx) = unbounded();
        let title_task = cx.spawn_in(window, async move |this, cx| {
            loop {
                let Ok((blocks, window_blocks, active, inputs)) =
                    this.update(cx, |this, cx| this.title_inputs(cx))
                else {
                    break;
                };
                // /proc reads and exec blocks may block, keep them off the main thread
                let (titles, window_title) = cx
                    .background_executor()
                    .spawn(async move {
                        let window_title = inputs
                            .get(active)
                            .map(|(_, inputs)| tab_title::build_title(&window_blocks, inputs))
                            .unwrap_or_default();
                        let titles = inputs
                            .into_iter()
                            .map(|(id, inputs)| (id, tab_title::build_title(&blocks, &inputs)))
                            .collect::<Vec<_>>();
                        (titles, window_title)
                    })
                    .await;
                let Ok(()) = this.update_in(cx, |this, window, cx| {
                    this.set_titles(titles, window_title, window, cx)
                }) else {
                    break;
                };
                let mut timer = cx
                    .background_executor()
                    .timer(TITLE_REFRESH_INTERVAL)
                    .fuse();
                futures::select_biased! {
                    _ = refresh_rx.next() => {},
                    _ = timer => {},
                }
                // many requests while building count as one
                while refresh_rx.try_recv().is_ok() {}
            }
        });
        let mut this = Self {
            tabs: Vec::new(),
            active: 0,
            tab_scroll: ScrollHandle::new(),
            window_title: String::new(),
            refresh_titles,
            _title_task: title_task,
        };
        cx.observe_window_appearance(window, |_, window, cx| {
            Theme::apply(window.appearance(), cx);
            // terminal views may be cached, force redraw everything with new colors
            window.refresh();
        })
        .detach();
        this.add_tab(window, cx);
        this
    }

    fn add_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let window_id = window.window_handle().window_id().as_u64();
        let builder = match TerminalBuilder::new(&Settings::get(cx).terminal, window_id) {
            Ok(builder) => builder,
            Err(error) => {
                eprintln!("failed to spawn terminal: {error:#}");
                return;
            }
        };
        let terminal = cx.new(|cx| builder.subscribe(cx));
        let view = cx.new(|cx| TerminalView::new(terminal.clone(), window, cx));

        let subscription = cx.subscribe_in(&terminal, window, {
            let view = view.clone();
            move |this, _, event: &Event, window, cx| match event {
                Event::TitleChanged => this.refresh_titles(),
                // shell exited, so close its tab
                // TODO: do we need it at all? probably we need a setting, if to keep exited tab..
                Event::CloseTerminal => {
                    if let Some(ix) = this.tabs.iter().position(|tab| tab.view == view) {
                        this.close_tab_at(ix, window, cx);
                    }
                }
                Event::Wakeup => {}
            }
        });

        self.tabs.push(Tab {
            view,
            title: String::new(),
            _subscription: subscription,
        });
        self.refresh_titles();
        self.activate_tab(self.tabs.len() - 1, window, cx);
    }

    fn activate_tab(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.active = ix;
        // window title follows the active tab
        self.refresh_titles();
        self.tab_scroll.scroll_to_item(ix);
        self.tabs[ix].view.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn close_tab_at(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.remove(ix);
        if self.tabs.is_empty() {
            cx.quit();
            return;
        }
        // tab numbers after the closed one shift
        self.refresh_titles();
        // closing active tab activates one on its left
        // TODO: should be configurable
        let active = if ix < self.active || (ix == self.active && ix > 0) {
            self.active - 1
        } else {
            self.active.min(self.tabs.len() - 1)
        };
        self.activate_tab(active, window, cx);
    }

    fn refresh_titles(&self) {
        self.refresh_titles.unbounded_send(()).ok();
    }

    fn title_inputs(&self, cx: &App) -> TitleSnapshot {
        let settings = Settings::get(cx);
        let inputs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(ix, tab)| {
                let terminal = tab.view.read(cx).terminal().read(cx);
                let inputs = TitleInputs {
                    number: ix + 1,
                    shell_pid: terminal.shell_pid,
                    title: terminal.title(&settings.default_title),
                };
                (tab.view.entity_id(), inputs)
            })
            .collect();
        (
            settings.tab_title.clone(),
            settings.window_title.clone(),
            self.active,
            inputs,
        )
    }

    fn set_titles(
        &mut self,
        titles: Vec<(EntityId, String)>,
        window_title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let window_title = if window_title.is_empty() {
            Settings::get(cx).default_title.clone()
        } else {
            window_title
        };
        // linux can't read the title back, so the last one is kept here
        if self.window_title != window_title {
            window.set_window_title(&window_title);
            self.window_title = window_title;
        }
        let mut changed = false;
        for (id, title) in titles {
            // tabs closed while building are skipped
            if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.view.entity_id() == id)
                && tab.title != title
            {
                tab.title = title;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.add_tab(window, cx);
    }

    fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab_at(self.active, window, cx);
    }

    fn next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        // wraps from the last tab back to the first
        self.activate_tab((self.active + 1) % self.tabs.len(), window, cx);
    }

    fn activate_tab_action(
        &mut self,
        action: &ActivateTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action.0 < self.tabs.len() {
            self.activate_tab(action.0, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Instant};

    use alacritty_terminal::term::TermMode;
    use gpui::{
        Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
        TestAppContext, VisualTestContext, point,
    };

    use super::*;
    use crate::{settings::Keybindings, terminal::Terminal};

    /// open a workspace with `tabs` tabs and user keybindings `keys`, the last tab is active
    fn open_with<'a>(
        cx: &'a mut TestAppContext,
        tabs: usize,
        keys: &str,
    ) -> (Entity<Workspace>, &'a mut VisualTestContext) {
        let keybindings = Keybindings::parse(keys).unwrap();
        // shells wake gpui tasks from alacritty's pty thread, which the test scheduler
        // only tolerates with parking allowed
        cx.executor().allow_parking();
        cx.update(|cx| {
            cx.set_global(Settings::default());
            // bundled theme, so no theme files are created
            cx.set_global(Theme::default());
            cx.bind_keys(keybindings.bindings());
        });
        let (ws, cx) = cx.add_window_view(Workspace::new);
        cx.run_until_parked();
        for _ in 1..tabs {
            ws.update_in(cx, |ws, window, cx| ws.add_tab(window, cx));
            cx.run_until_parked();
        }
        assert_eq!(
            ws.update(cx, |ws, _| ws.tabs.len()),
            tabs,
            "failed to spawn shells"
        );
        (ws, cx)
    }

    fn open(cx: &mut TestAppContext, tabs: usize) -> (Entity<Workspace>, &mut VisualTestContext) {
        open_with(cx, tabs, "{}")
    }

    fn views(ws: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<EntityId> {
        ws.update(cx, |ws, _| {
            ws.tabs.iter().map(|t| t.view.entity_id()).collect()
        })
    }

    /// active tab index, checking that focus is on it
    fn current(ws: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
        ws.update_in(cx, |ws, window, cx| {
            let focused: Vec<usize> = ws
                .tabs
                .iter()
                .enumerate()
                .filter(|(_, tab)| tab.view.focus_handle(cx).is_focused(window))
                .map(|(ix, _)| ix)
                .collect();
            assert_eq!(
                focused,
                vec![ws.active],
                "focus does not follow the active tab"
            );
            ws.active
        })
    }

    fn next(ws: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
        ws.update_in(cx, |ws, window, cx| ws.next_tab(&NextTab, window, cx));
        cx.run_until_parked();
        current(ws, cx)
    }

    fn activate(ws: &Entity<Workspace>, cx: &mut VisualTestContext, ix: usize) -> usize {
        ws.update_in(cx, |ws, window, cx| {
            ws.activate_tab_action(&ActivateTab(ix), window, cx)
        });
        cx.run_until_parked();
        current(ws, cx)
    }

    fn keys(ws: &Entity<Workspace>, cx: &mut VisualTestContext, keystrokes: &str) -> usize {
        cx.run_until_parked();
        cx.simulate_keystrokes(keystrokes);
        cx.run_until_parked();
        current(ws, cx)
    }

    #[gpui::test]
    fn next_tab_wraps_from_last_to_first(cx: &mut TestAppContext) {
        let (ws, cx) = open(cx, 3);
        assert_eq!(current(&ws, cx), 2);
        assert_eq!(next(&ws, cx), 0);
    }

    #[gpui::test]
    fn activate_tab_out_of_range_is_ignored(cx: &mut TestAppContext) {
        let (ws, cx) = open(cx, 3);
        activate(&ws, cx, 1);
        let before = views(&ws, cx);
        for ix in [3, 4, 8, 9, 99, usize::MAX - 1, usize::MAX] {
            assert_eq!(activate(&ws, cx, ix), 1, "index {ix}");
        }
        assert_eq!(views(&ws, cx), before);
    }

    #[gpui::test]
    fn activate_tab_after_close_uses_shifted_indexes(cx: &mut TestAppContext) {
        let (ws, cx) = open(cx, 4);
        let old = views(&ws, cx);
        ws.update_in(cx, |ws, window, cx| ws.close_tab_at(1, window, cx));
        cx.run_until_parked();
        // index 1 is now the old third tab, index 3 is gone
        assert_eq!(activate(&ws, cx, 1), 1);
        assert_eq!(views(&ws, cx)[1], old[2]);
        assert_eq!(activate(&ws, cx, 3), 1);
        assert_eq!(activate(&ws, cx, 2), 2);
    }

    #[gpui::test]
    fn disabled_next_tab_ignores_ctrl_tab(cx: &mut TestAppContext) {
        let (ws, cx) = open_with(cx, 3, r#"{"next_tab": null}"#);
        assert_eq!(keys(&ws, cx, "ctrl-tab"), 2);
        assert_eq!(keys(&ws, cx, "alt-1"), 0);
    }

    // raw input bytes a focus reader collects before it ends
    const READ: usize = 30;

    /// let the pty threads run, then move the fake clock so batched alacritty events flow
    fn pump(cx: &mut VisualTestContext) {
        std::thread::sleep(Duration::from_millis(15));
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
    }

    fn wait_until(
        cx: &mut VisualTestContext,
        what: &str,
        mut done: impl FnMut(&mut VisualTestContext) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !done(cx) {
            assert!(Instant::now() < deadline, "timed out: {what}");
            pump(cx);
        }
    }

    // gpui test windows start inactive, activating is like a user click
    fn set_window_active(cx: &mut VisualTestContext, active: bool) {
        if active {
            cx.update(|window, _| window.activate_window());
        } else {
            cx.deactivate_window();
        }
        cx.run_until_parked();
        pump(cx);
    }

    fn terminal(ws: &Entity<Workspace>, cx: &mut VisualTestContext, ix: usize) -> Entity<Terminal> {
        ws.update(cx, |ws, cx| ws.tabs[ix].view.read(cx).terminal().clone())
    }

    /// in the active tab: optionally enable focus reporting (mode 1004), then save READ raw
    /// input bytes as hex into the returned file
    fn reader(
        ws: &Entity<Workspace>,
        cx: &mut VisualTestContext,
        focus_mode: bool,
        name: &str,
    ) -> (usize, PathBuf) {
        let file =
            std::env::temp_dir().join(format!("kuterm_focus_{name}_{}", std::process::id()));
        let ready = file.with_extension("ready");
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_file(&ready);
        let mode = if focus_mode {
            "printf '\\033[?1004h'; "
        } else {
            ""
        };
        let (f, r) = (file.display(), ready.display());
        let command = format!(
            "{mode}stty raw -echo; touch {r}; dd bs=1 count={READ} 2>/dev/null | od -An -v -tx1 | tr -d ' \\n' > {f}.tmp; mv {f}.tmp {f}; stty sane\r"
        );
        let ix = current(ws, cx);
        let terminal = terminal(ws, cx, ix);
        terminal.update(cx, |t, _| t.input(command.into_bytes()));
        wait_until(cx, "reader never started", |_| ready.exists());
        wait_until(cx, "focus mode never reached the view", |cx| {
            terminal.read_with(cx, |t, _| {
                t.last_content.mode.contains(TermMode::FOCUS_IN_OUT)
            }) == focus_mode
        });
        // let dd start reading
        for _ in 0..5 {
            pump(cx);
        }
        std::fs::remove_file(&ready).ok();
        (ix, file)
    }

    fn send_to(ws: &Entity<Workspace>, cx: &mut VisualTestContext, ix: usize, text: &str) {
        let bytes = text.as_bytes().to_vec();
        terminal(ws, cx, ix).update(cx, |t, _| t.input(bytes));
        pump(cx);
    }

    /// fill the reader up and return what it got before: "I", "O" or other bytes as hex, with
    /// repeated identical reports merged
    fn finish(
        ws: &Entity<Workspace>,
        cx: &mut VisualTestContext,
        (ix, file): (usize, PathBuf),
    ) -> Vec<String> {
        send_to(ws, cx, ix, &"x".repeat(READ));
        wait_until(cx, "no reader output", |_| file.exists());
        let hex = std::fs::read_to_string(&file).unwrap();
        std::fs::remove_file(&file).ok();
        let mut tokens: Vec<String> = Vec::new();
        let mut rest = hex.as_str();
        while !rest.is_empty() {
            if let Some(r) = rest.strip_prefix("1b5b49") {
                tokens.push("I".into());
                rest = r;
            } else if let Some(r) = rest.strip_prefix("1b5b4f") {
                tokens.push("O".into());
                rest = r;
            } else {
                tokens.push(rest[..2].into());
                rest = &rest[2..];
            }
        }
        while tokens.last().is_some_and(|t| t == "78") {
            tokens.pop();
        }
        // gpui's focus listeners may fire twice for one activation change
        tokens.dedup_by(|a, b| a == b && (a == "I" || a == "O"));
        tokens
    }

    #[gpui::test]
    fn focus_mode_reports_out_on_deactivate_and_in_on_activate(cx: &mut TestAppContext) {
        let (ws, cx) = open(cx, 1);
        set_window_active(cx, true);
        let r = reader(&ws, cx, true, "report");
        set_window_active(cx, false);
        set_window_active(cx, true);
        assert_eq!(finish(&ws, cx, r), ["O", "I"]);
    }

    #[gpui::test]
    fn no_reports_without_focus_mode(cx: &mut TestAppContext) {
        let (ws, cx) = open(cx, 1);
        set_window_active(cx, true);
        let r = reader(&ws, cx, false, "off");
        for _ in 0..3 {
            set_window_active(cx, false);
            set_window_active(cx, true);
        }
        send_to(&ws, cx, 0, "abc");
        assert_eq!(finish(&ws, cx, r), ["61", "62", "63"]);
    }

    #[gpui::test]
    fn only_the_active_tab_reports_window_activation(cx: &mut TestAppContext) {
        for active in 0..3 {
            let (ws, cx) = open(cx, 3);
            set_window_active(cx, true);
            let mut readers = Vec::new();
            for ix in 0..3 {
                activate(&ws, cx, ix);
                readers.push(reader(&ws, cx, true, &format!("tab{active}_{ix}")));
            }
            activate(&ws, cx, active);
            set_window_active(cx, false);
            set_window_active(cx, true);
            // tabs 0 and 1 lost focus to the next tab during setup, picking `active` moves focus
            // from tab 2, then only `active` sees the window
            let mut want: Vec<Vec<&str>> = vec![vec!["O"], vec!["O"], vec![]];
            if active != 2 {
                want[2].push("O");
                want[active].push("I");
            }
            want[active].extend(["O", "I"]);
            for (ix, r) in readers.into_iter().enumerate() {
                assert_eq!(
                    finish(&ws, cx, r),
                    want[ix],
                    "tab {ix} with tab {active} active"
                );
            }
        }
    }

    #[gpui::test]
    fn plain_ctrl_w_and_ctrl_t_reach_the_shell(cx: &mut TestAppContext) {
        let (ws, cx) = open(cx, 2);
        let r = reader(&ws, cx, false, "ctrl_keys");
        cx.simulate_keystrokes("ctrl-w ctrl-t");
        pump(cx);
        assert_eq!(views(&ws, cx).len(), 2, "a shell key changed the tabs");
        assert_eq!(finish(&ws, cx, r), ["17", "14"]);
    }

    #[gpui::test]
    fn ctrl_shift_t_and_ctrl_shift_w_manage_tabs(cx: &mut TestAppContext) {
        let (ws, cx) = open(cx, 1);
        keys(&ws, cx, "ctrl-shift-t");
        assert_eq!(views(&ws, cx).len(), 2);
        keys(&ws, cx, "ctrl-shift-w");
        assert_eq!(views(&ws, cx).len(), 1);
    }

    /// screen line that starts with `text`
    fn line_starting_with(terminal: &Terminal, text: &str) -> Option<usize> {
        let mut lines: Vec<String> = Vec::new();
        for indexed in &terminal.last_content.cells {
            if indexed.point.column.0 == 0 {
                lines.push(String::new());
            }
            lines.last_mut()?.push(indexed.cell.c);
        }
        lines.iter().position(|line| line.starts_with(text))
    }

    /// window position inside `column` of the screen line that starts with `text`, in the
    /// cell's left or right half
    fn cell_of(
        ws: &Entity<Workspace>,
        cx: &mut VisualTestContext,
        text: &str,
        column: usize,
        right: bool,
    ) -> Point<Pixels> {
        let x = column as f32 + if right { 0.8 } else { 0.2 };
        let ix = current(ws, cx);
        terminal(ws, cx, ix).read_with(cx, |t, _| {
            let line = line_starting_with(t, text)
                .unwrap_or_else(|| panic!("no line starts with {text:?}"));
            let b = t.last_content.terminal_bounds;
            point(
                b.bounds.origin.x + b.cell_width * x,
                b.bounds.origin.y + b.line_height * (line as f32 + 0.5),
            )
        })
    }

    fn print_line(ws: &Entity<Workspace>, cx: &mut VisualTestContext, text: &str) {
        let ix = current(ws, cx);
        // split with %s so only the output, not the typed command, starts with `text`
        let (head, tail) = text.split_at(2);
        send_to(
            ws,
            cx,
            ix,
            &format!("clear; printf '{head}%s\\n' '{tail}'\r"),
        );
        let terminal = terminal(ws, cx, ix);
        wait_until(cx, "text never printed", |cx| {
            terminal.read_with(cx, |t, _| line_starting_with(t, text).is_some())
        });
    }

    fn click(cx: &mut VisualTestContext, position: Point<Pixels>, click_count: usize) {
        cx.simulate_event(MouseDownEvent {
            position,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count,
            first_mouse: false,
        });
    }

    fn release(cx: &mut VisualTestContext, position: Point<Pixels>) {
        cx.simulate_event(MouseUpEvent {
            position,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
        });
    }

    fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
        cx.read_from_clipboard().and_then(|item| item.text())
    }

    #[gpui::test]
    fn mouse_drag_selects_and_ctrl_shift_c_copies(cx: &mut TestAppContext) {
        let (ws, cx) = open(cx, 1);
        print_line(&ws, cx, "pick these words");

        let start = cell_of(&ws, cx, "pick these words", 5, false);
        let end = cell_of(&ws, cx, "pick these words", 9, true);
        click(cx, start, 1);
        cx.simulate_event(MouseMoveEvent {
            position: end,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        });
        release(cx, end);
        cx.simulate_keystrokes("ctrl-shift-c");
        assert_eq!(clipboard(cx).as_deref(), Some("these"));

        // a move without the button held does not change the selection
        let hover = cell_of(&ws, cx, "pick these words", 14, true);
        cx.simulate_event(MouseMoveEvent {
            position: hover,
            pressed_button: None,
            modifiers: Modifiers::default(),
        });
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new()));
        cx.simulate_keystrokes("ctrl-shift-c");
        assert_eq!(clipboard(cx).as_deref(), Some("these"));
    }

    #[gpui::test]
    fn double_click_copies_a_word(cx: &mut TestAppContext) {
        let (ws, cx) = open(cx, 1);
        print_line(&ws, cx, "pick these words");
        let position = cell_of(&ws, cx, "pick these words", 12, false);
        click(cx, position, 1);
        release(cx, position);
        click(cx, position, 2);
        release(cx, position);
        cx.simulate_keystrokes("ctrl-shift-c");
        assert_eq!(clipboard(cx).as_deref(), Some("words"));
    }

    #[gpui::test]
    fn copy_without_selection_keeps_clipboard(cx: &mut TestAppContext) {
        let (_ws, cx) = open(cx, 1);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
        cx.simulate_keystrokes("ctrl-shift-c");
        assert_eq!(clipboard(cx).as_deref(), Some("before"));
    }
}
