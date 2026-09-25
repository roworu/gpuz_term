//! terminal model: runs shell and keeps screen state

mod bounds;
mod builder;
mod content;
mod events;
mod keys;
#[cfg(test)]
mod tests;

use std::{borrow::Cow, sync::Arc};

use alacritty_terminal::{
    Term,
    event::Notify,
    event_loop::{Msg, Notifier},
    grid::Scroll,
    sync::FairMutex,
    term::TermMode,
};
use gpui::{EventEmitter, Keystroke, Task};

pub use bounds::TerminalBounds;
pub use builder::TerminalBuilder;
pub use content::{Content, IndexedCell};

use builder::ZedListener;
use content::make_content;
use keys::to_esc_str;

/// events emitted to terminal view
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    TitleChanged,
    Wakeup,
    CloseTerminal,
}

enum InternalEvent {
    Resize(TerminalBounds),
    Scroll(Scroll),
}

pub struct Terminal {
    pty_tx: Notifier,
    term: Arc<FairMutex<Term<ZedListener>>>,
    events: Vec<InternalEvent>,
    pub last_content: Content,
    title: String,
    _event_loop_task: Task<()>,
}

impl EventEmitter<Event> for Terminal {}
impl Terminal {
    /// title set by the running program, falls back to "Terminal"
    /// TODO: need settings for that..
    pub fn title(&self) -> String {
        if self.title.is_empty() {
            "Terminal".to_string()
        } else {
            self.title.clone()
        }
    }

    fn write_to_pty(&self, input: impl Into<Cow<'static, [u8]>>) {
        self.pty_tx.notify(input.into());
    }

    /// send user input and jump back to bottom of scrollback
    pub fn input(&mut self, input: impl Into<Cow<'static, [u8]>>) {
        self.events.push(InternalEvent::Scroll(Scroll::Bottom));
        self.write_to_pty(input);
    }

    /// queue a resize, applied on next `sync`
    pub fn set_size(&mut self, new_bounds: TerminalBounds) {
        let old_bounds = self.last_content.terminal_bounds;
        self.last_content.terminal_bounds = new_bounds;
        // skip pixel-only changes so dragging window does not spam SIGWINCH
        if old_bounds.num_lines() == new_bounds.num_lines()
            && old_bounds.num_columns() == new_bounds.num_columns()
        {
            return;
        }
        self.events.push(InternalEvent::Resize(new_bounds));
    }

    /// scroll the viewport by lines, positive is up into history
    pub fn scroll(&mut self, lines: i32) {
        self.events.push(InternalEvent::Scroll(Scroll::Delta(lines)));
    }

    /// map a keystroke to an escape sequence and write it, returns false if unmapped
    pub fn try_keystroke(&mut self, keystroke: &Keystroke) -> bool {
        match to_esc_str(keystroke, self.last_content.mode, false) {
            Some(Cow::Borrowed(esc)) => self.input(esc.as_bytes()),
            Some(Cow::Owned(esc)) => self.input(esc.into_bytes()),
            None => return false,
        }
        true
    }

    /// paste text, wrapping it when program asked for bracketed paste
    pub fn paste(&mut self, text: &str) {
        let text = if self.last_content.mode.contains(TermMode::BRACKETED_PASTE) {
            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r")
        };
        self.input(text.into_bytes());
    }

    /// report focus changes to programs that ask for that
    pub fn focus_changed(&self, focused: bool) {
        if self.last_content.mode.contains(TermMode::FOCUS_IN_OUT) {
            self.write_to_pty(if focused { "\x1b[I" } else { "\x1b[O" }.as_bytes());
        }
    }

    /// apply queued events and take a fresh snapshot of grid
    pub fn sync(&mut self) {
        let term = self.term.clone();
        let mut term = term.lock_unfair();
        for event in self.events.drain(..) {
            match event {
                InternalEvent::Resize(bounds) => {
                    self.pty_tx.0.send(Msg::Resize(bounds.into())).ok();
                    term.resize(bounds);
                }
                InternalEvent::Scroll(scroll) => term.scroll_display(scroll),
            }
        }
        self.last_content = make_content(&term, self.last_content.terminal_bounds);
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.pty_tx.0.send(Msg::Shutdown).ok();
    }
}
