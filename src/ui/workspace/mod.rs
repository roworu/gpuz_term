//! root view: owns tabs and handles new tab / close tab.

mod tab_bar;

use gpui::{Context, Entity, Focusable, Subscription, Window, actions, prelude::*};

use crate::{
    settings::Settings,
    terminal::{Event, TerminalBuilder},
    ui::terminal_view::TerminalView,
};

actions!(workspace, [NewTab, CloseTab]);

struct Tab {
    view: Entity<TerminalView>,
    _subscription: Subscription,
}

pub struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
}

impl Workspace {
    /// create workspace with one terminal tab
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            tabs: Vec::new(),
            active: 0,
        };
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
                Event::TitleChanged => cx.notify(),
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
            _subscription: subscription,
        });
        self.activate_tab(self.tabs.len() - 1, window, cx);
    }

    fn activate_tab(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.active = ix;
        self.tabs[ix].view.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn close_tab_at(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.remove(ix);
        if self.tabs.is_empty() {
            cx.quit();
            return;
        }
        // closing active tab activates one on its left
        // TODO: should be configurable
        let active = if ix < self.active || (ix == self.active && ix > 0) {
            self.active - 1
        } else {
            self.active.min(self.tabs.len() - 1)
        };
        self.activate_tab(active, window, cx);
    }

    fn new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.add_tab(window, cx);
    }

    fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab_at(self.active, window, cx);
    }
}
