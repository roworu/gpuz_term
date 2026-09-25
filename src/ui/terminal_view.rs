//! focusable view around terminal

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Pixels, Render, ScrollDelta, ScrollWheelEvent, Styled, Subscription, Window,
    actions, div, px,
};

use crate::{
    terminal::{Event, Terminal},
    ui::terminal_element::TerminalElement,
};

actions!(terminal, [Paste]);

// default terminal scroll_multiplier
const SCROLL_MULTIPLIER: f32 = 2.;

pub struct TerminalView {
    terminal: Entity<Terminal>,
    focus_handle: FocusHandle,
    scroll_px: Pixels,
    _subscriptions: Vec<Subscription>,
}

impl TerminalView {
    /// create a view that renders and forwards input to the terminal
    pub fn new(terminal: Entity<Terminal>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let subscriptions = vec![
            cx.subscribe(&terminal, |_, _, event: &Event, cx| {
                if *event == Event::Wakeup {
                    cx.notify();
                }
            }),
            cx.on_focus_in(&focus_handle, window, |this, _, cx| {
                this.terminal.read(cx).focus_changed(true);
                cx.notify();
            }),
            cx.on_focus_out(&focus_handle, window, |this, _, _, cx| {
                this.terminal.read(cx).focus_changed(false);
                cx.notify();
            }),
        ];
        Self {
            terminal,
            focus_handle,
            scroll_px: px(0.),
            _subscriptions: subscriptions,
        }
    }

    /// terminal model this view renders
    pub fn terminal(&self) -> &Entity<Terminal> {
        &self.terminal
    }

    /// send committed text from  input handler to pty
    pub fn commit_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if !text.is_empty() {
            self.terminal
                .update(cx, |term, _| term.input(text.to_string().into_bytes()));
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        // let input handler receive layout-dependent characters, e.g. altgr on windows
        if event.prefer_character_input && event.keystroke.key_char.is_some() {
            return;
        }
        if self
            .terminal
            .update(cx, |term, _| term.try_keystroke(&event.keystroke))
        {
            cx.stop_propagation();
        }
    }

    fn scroll_wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let line_height = self.terminal.read(cx).last_content.terminal_bounds.line_height;
        let lines = match event.delta {
            ScrollDelta::Lines(delta) => (delta.y * SCROLL_MULTIPLIER) as i32,
            ScrollDelta::Pixels(delta) => {
                // accumulate touchpad pixels until they add up to whole lines
                self.scroll_px += delta.y * SCROLL_MULTIPLIER;
                let lines = (self.scroll_px / line_height) as i32;
                self.scroll_px -= line_height * lines as f32;
                lines
            }
        };
        if lines != 0 {
            self.terminal.update(cx, |term, _| term.scroll(lines));
            cx.notify();
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.terminal.update(cx, |term, _| term.paste(&text));
        }
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        div()
            .id("terminal-view")
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("Terminal")
            .on_action(cx.listener(Self::paste))
            .on_key_down(cx.listener(Self::key_down))
            .on_scroll_wheel(cx.listener(Self::scroll_wheel))
            .child(TerminalElement::new(
                self.terminal.clone(),
                cx.entity(),
                self.focus_handle.clone(),
                focused,
            ))
    }
}
