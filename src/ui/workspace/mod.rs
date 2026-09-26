//! root view: owns tabs and handles new tab / close tab.

mod tab_bar;
mod tab_title;

use std::time::Duration;

use futures::{
    FutureExt, StreamExt,
    channel::mpsc::{UnboundedSender, unbounded},
};
use gpui::{
    App, Context, Entity, EntityId, Focusable, ScrollHandle, Subscription, Task, Window, actions,
    prelude::*,
};

use tab_title::TitleInputs;

use crate::{
    settings::{Settings, TabTitleBlock},
    terminal::{Event, TerminalBuilder},
    theme::Theme,
    ui::terminal_view::TerminalView,
};

actions!(workspace, [NewTab, CloseTab]);

// programs, folders and command output change without events, so titles are polled
const TITLE_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

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
    refresh_titles: UnboundedSender<()>,
    _title_task: Task<()>,
}

impl Workspace {
    /// create workspace with one terminal tab
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (refresh_titles, mut refresh_rx) = unbounded();
        let title_task = cx.spawn(async move |this, cx| {
            loop {
                let Ok((blocks, inputs)) = this.update(cx, |this, cx| this.title_inputs(cx)) else {
                    break;
                };
                // /proc reads and exec blocks may block, keep them off the main thread
                let titles = cx
                    .background_executor()
                    .spawn(async move {
                        inputs
                            .into_iter()
                            .map(|(id, inputs)| (id, tab_title::build_title(&blocks, &inputs)))
                            .collect::<Vec<_>>()
                    })
                    .await;
                let Ok(()) = this.update(cx, |this, cx| this.set_titles(titles, cx)) else {
                    break;
                };
                let mut timer = cx.background_executor().timer(TITLE_REFRESH_INTERVAL).fuse();
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

    fn title_inputs(&self, cx: &App) -> (Vec<TabTitleBlock>, Vec<(EntityId, TitleInputs)>) {
        let inputs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(ix, tab)| {
                let terminal = tab.view.read(cx).terminal().read(cx);
                let inputs = TitleInputs {
                    number: ix + 1,
                    shell_pid: terminal.shell_pid,
                    title: terminal.title(),
                };
                (tab.view.entity_id(), inputs)
            })
            .collect();
        (Settings::get(cx).tab_title.clone(), inputs)
    }

    fn set_titles(&mut self, titles: Vec<(EntityId, String)>, cx: &mut Context<Self>) {
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
}
