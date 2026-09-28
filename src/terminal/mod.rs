//! terminal model: runs shell and keeps screen state

mod bounds;
mod builder;
mod content;
mod events;
mod keys;
mod process;

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
pub use process::foreground_process;

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
    /// pid of the shell running in the pty
    pub shell_pid: u32,
    _event_loop_task: Task<()>,
}

impl EventEmitter<Event> for Terminal {}
impl Terminal {
    /// title set by the running program, falls back to `default_title`
    pub fn title(&self, default_title: &str) -> String {
        if self.title.is_empty() {
            default_title.to_string()
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
        self.events
            .push(InternalEvent::Scroll(Scroll::Delta(lines)));
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui::point;

    use alacritty_terminal::event::Event as AlacTermEvent;
    use futures::{FutureExt, StreamExt};
    use gpui::{Bounds, px, size};

    use super::{
        Terminal, TerminalBounds, TerminalBuilder, foreground_process, process::ForegroundProcess,
    };
    use crate::settings::{CursorShape, Shell, TerminalSettings};

    fn spawn(settings: &TerminalSettings) -> TerminalBuilder {
        let mut builder = TerminalBuilder::new(settings, 0).expect("failed to spawn shell");
        // 80x24 grid, a real window would size it in prepaint
        builder.terminal.set_size(TerminalBounds::new(
            px(20.),
            px(10.),
            Bounds::new(point(px(0.), px(0.)), size(px(800.), px(480.))),
        ));
        builder.terminal.sync();
        builder
    }

    fn screen_text(terminal: &Terminal) -> String {
        let mut text = String::new();
        let mut last_line = None;
        for indexed in &terminal.last_content.cells {
            if last_line.is_some_and(|line| line != indexed.point.line) {
                text.push('\n');
            }
            last_line = Some(indexed.point.line);
            text.push(indexed.cell.c);
        }
        text
    }

    fn wait_for_text(terminal: &mut Terminal, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            terminal.sync();
            let text = screen_text(terminal);
            if text.contains(needle) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "no {needle:?} on screen:\n{text}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn shell_runs_input() {
        let mut builder = spawn(&TerminalSettings::default());
        builder.terminal.input(b"echo out_$((6*7))\r".to_vec());
        wait_for_text(&mut builder.terminal, "out_42");
    }

    #[test]
    fn configured_shell_is_launched() {
        let settings = TerminalSettings {
            shell: Shell::WithArguments {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), "echo from_settings_$((2+3)); sleep 5".into()],
            },
            ..TerminalSettings::default()
        };
        let mut builder = spawn(&settings);
        wait_for_text(&mut builder.terminal, "from_settings_5");
    }

    #[test]
    fn configured_cursor_shape_is_used() {
        let settings = TerminalSettings {
            cursor_shape: CursorShape::Bar,
            ..TerminalSettings::default()
        };
        let builder = spawn(&settings);
        assert_eq!(
            builder.terminal.last_content.cursor.shape,
            alacritty_terminal::vte::ansi::CursorShape::Beam
        );
    }

    #[test]
    fn resize_reaches_shell() {
        let mut builder = spawn(&TerminalSettings::default());
        builder.terminal.set_size(TerminalBounds::new(
            px(20.),
            px(10.),
            Bounds::new(point(px(0.), px(0.)), size(px(1000.), px(500.))),
        ));
        builder.terminal.input(b"echo size=$(stty size)\r".to_vec());
        wait_for_text(&mut builder.terminal, "size=25 100");
    }

    #[test]
    fn exit_emits_exit_event() {
        let mut builder = spawn(&TerminalSettings::default());
        builder.terminal.input(b"exit\r".to_vec());
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            while let Some(event) = builder.events_rx.next().now_or_never() {
                match event {
                    Some(AlacTermEvent::Exit | AlacTermEvent::ChildExit(_)) => return,
                    None => panic!("event channel closed without an exit event"),
                    Some(_) => {}
                }
            }
            assert!(Instant::now() < deadline, "no exit event after `exit`");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn foreground_process_follows_shell() {
        let mut builder = spawn(&TerminalSettings::default());
        builder.terminal.input(b"cd /tmp && sleep 5\r".to_vec());
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let process = foreground_process(builder.terminal.shell_pid);
            if let Some(ForegroundProcess { name, cwd }) = &process
                && name == "sleep"
            {
                assert_eq!(cwd.as_deref(), Some(std::path::Path::new("/tmp")));
                return;
            }
            assert!(
                Instant::now() < deadline,
                "sleep is not in foreground: {process:?}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
