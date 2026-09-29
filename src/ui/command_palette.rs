//! command palette and the about page it opens, both float over the terminal

use gpui::{
    App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, KeyDownEvent, ScrollHandle,
    Window, div, prelude::*, px, rems,
};

use crate::{settings::Command, theme::Theme};

/// true when every query letter shows up in `name` in order, ignoring case and spaces
fn fuzzy_match(name: &str, query: &str) -> bool {
    let mut name = name.chars().flat_map(char::to_lowercase);
    query
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .all(|q| name.any(|ch| ch == q))
}

/// floating box near the top of the window, shared by the palette and the about page
fn panel(theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .w(rems(34.))
        .max_w_full()
        .bg(theme.tab_bar_background)
        .border_1()
        .border_color(theme.border)
        .rounded_md()
        .shadow_lg()
        .text_color(theme.text)
        .overflow_hidden()
}

/// list of commands filtered by typed text, emits the picked command
pub struct CommandPalette {
    focus_handle: FocusHandle,
    commands: Vec<Command>,
    query: String,
    /// index into `matches()`
    selected: usize,
    scroll: ScrollHandle,
}

impl EventEmitter<DismissEvent> for CommandPalette {}
impl EventEmitter<Command> for CommandPalette {}

impl CommandPalette {
    /// palette listing `commands` with an empty query
    pub fn new(commands: Vec<Command>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            commands,
            query: String::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
        }
    }

    /// commands whose name matches the query, in file order
    pub fn matches(&self) -> Vec<&Command> {
        self.commands
            .iter()
            .filter(|command| fuzzy_match(&command.name, &self.query))
            .collect()
    }

    fn select(&mut self, ix: usize) {
        self.selected = ix;
        self.scroll.scroll_to_item(ix);
    }

    fn confirm(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(command) = self.matches().get(ix).map(|command| (*command).clone()) {
            cx.emit(command);
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let count = self.matches().len();
        match keystroke.key.as_str() {
            "escape" => cx.emit(DismissEvent),
            "enter" => self.confirm(self.selected, cx),
            "up" => self.select(self.selected.saturating_sub(1)),
            "down" => self.select((self.selected + 1).min(count.saturating_sub(1))),
            "backspace" => {
                self.query.pop();
                self.select(0);
            }
            _ => {
                let m = keystroke.modifiers;
                match &keystroke.key_char {
                    // tab and friends carry control characters, they are not part of a name
                    Some(text)
                        if !m.control
                            && !m.platform
                            && !m.function
                            && !text.chars().any(char::is_control) =>
                    {
                        self.query.push_str(text);
                        self.select(0);
                    }
                    _ => return,
                }
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Focusable for CommandPalette {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CommandPalette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::get(cx);
        let input = if self.query.is_empty() {
            div()
                .text_color(theme.text_muted)
                .child("execute a command...")
        } else {
            div().child(self.query.clone())
        };
        let items = self
            .matches()
            .into_iter()
            .enumerate()
            .map(|(ix, command)| {
                div()
                    .id(("command", ix))
                    .debug_selector(move || format!("command-{ix}"))
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .when(ix == self.selected, |item| {
                        item.bg(theme.tab_active_background)
                    })
                    .hover(|item| item.bg(theme.tab_active_background))
                    .child(command.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| this.confirm(ix, cx)))
            })
            .collect::<Vec<_>>();
        let list = if items.is_empty() {
            div()
                .px_3()
                .py_2()
                .text_color(theme.text_muted)
                .child("no matching commands")
        } else {
            div().p_1().children(items)
        };
        panel(theme)
            .id("command-palette")
            .debug_selector(|| "command-palette".into())
            .track_focus(&self.focus_handle)
            .key_context("CommandPalette")
            .on_key_down(cx.listener(Self::key_down))
            // keeps clicks from reaching the terminal below
            .occlude()
            .on_mouse_down_out(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(input)
                    // stands in for a text cursor
                    .child(div().ml_0p5().w(px(2.)).h(rems(1.)).bg(theme.cursor)),
            )
            .child(
                div()
                    .id("command-list")
                    .max_h(rems(20.))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(list),
            )
    }
}

/// page with the version and links, closed by any key or a click outside
pub struct About {
    focus_handle: FocusHandle,
}

impl EventEmitter<DismissEvent> for About {}

impl About {
    /// about page ready to be focused
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Focusable for About {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for About {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::get(cx);
        let line = |label: &'static str, value: &'static str| {
            div()
                .flex()
                .gap_2()
                .child(div().w(rems(6.)).text_color(theme.text_muted).child(label))
                .child(value)
        };
        panel(theme)
            .id("about")
            .debug_selector(|| "about".into())
            .track_focus(&self.focus_handle)
            .key_context("About")
            .on_key_down(cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
                cx.emit(DismissEvent);
            }))
            .occlude()
            .on_mouse_down_out(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
            .p_4()
            .gap_1()
            .child(div().text_size(rems(1.5)).child("kuterm"))
            .child(
                div()
                    .pb_2()
                    .text_color(theme.text_muted)
                    .child(env!("CARGO_PKG_DESCRIPTION")),
            )
            .child(line("version", env!("CARGO_PKG_VERSION")))
            .child(line("source", env!("CARGO_PKG_REPOSITORY")))
            .child(line("license", env!("CARGO_PKG_LICENSE")))
            .child(
                div()
                    .pt_2()
                    .text_color(theme.text_muted)
                    .child("press any key to close"),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_match_skips_letters_case_and_spaces() {
        assert!(fuzzy_match("reload settings", ""));
        assert!(fuzzy_match("reload settings", "rel set"));
        assert!(fuzzy_match("reload settings", "RLDSTG"));
        assert!(!fuzzy_match("reload settings", "settings reload"));
        assert!(!fuzzy_match("about", "abx"));
    }
}
