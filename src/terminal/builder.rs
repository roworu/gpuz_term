//! spawning shell and pumping alacritty's events into terminal

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use alacritty_terminal::{
    Term,
    event::{Event as AlacTermEvent, EventListener},
    event_loop::{EventLoop, Notifier},
    sync::FairMutex,
    term::Config,
    tty,
    vte::ansi::CursorStyle,
};
use anyhow::{Context as _, Result};
use futures::{
    FutureExt, StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
};
use gpui::{Context, Task};

use super::{Content, Terminal, TerminalBounds};
use crate::settings::{Shell, TerminalSettings};

const DEFAULT_SCROLL_HISTORY_LINES: usize = 10_000;

#[derive(Clone)]
pub(super) struct ZedListener {
    events: UnboundedSender<AlacTermEvent>,
    /// shared with `Terminal`, lets `sync` skip snapshots while the grid is unchanged
    dirty: Arc<AtomicBool>,
}

impl EventListener for ZedListener {
    fn send_event(&self, event: AlacTermEvent) {
        // set before the event is queued, so the redraw it triggers always sees the flag
        if matches!(event, AlacTermEvent::Wakeup) {
            self.dirty.store(true, Ordering::Release);
        }
        self.events.unbounded_send(event).ok();
    }
}

pub struct TerminalBuilder {
    pub(super) terminal: Terminal,
    pub(super) events_rx: UnboundedReceiver<AlacTermEvent>,
}

impl TerminalBuilder {
    /// spawn configured shell in a new pty
    pub fn new(settings: &TerminalSettings, window_id: u64) -> Result<TerminalBuilder> {
        let mut env = HashMap::new();
        if std::env::var("LANG").is_err() {
            env.insert("LANG".to_string(), "en_US.UTF-8".to_string());
        }
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        env.insert("COLORTERM".to_string(), "truecolor".to_string());
        env.insert("TERM_PROGRAM".to_string(), "kuterm".to_string());
        env.insert(
            "TERM_PROGRAM_VERSION".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        );

        let shell = match &settings.shell {
            Shell::System => None,
            Shell::Program(program) => Some(tty::Shell::new(program.clone(), Vec::new())),
            Shell::WithArguments { program, args } => {
                Some(tty::Shell::new(program.clone(), args.clone()))
            }
        };
        let pty_options = tty::Options {
            shell,
            drain_on_exit: true,
            env,
            ..Default::default()
        };

        let config = Config {
            scrolling_history: DEFAULT_SCROLL_HISTORY_LINES,
            default_cursor_style: CursorStyle {
                shape: settings.cursor_shape.into(),
                blinking: false,
            },
            ..Config::default()
        };

        // alacritty's event loop talks to us with that channel
        let (events_tx, events_rx) = unbounded();
        let dirty = Arc::new(AtomicBool::new(true));
        let listener = ZedListener {
            events: events_tx,
            dirty: dirty.clone(),
        };
        let term = Term::new(config, &TerminalBounds::default(), listener.clone());
        let term = Arc::new(FairMutex::new(term));

        let pty = tty::new(&pty_options, TerminalBounds::default().into(), window_id)
            .context("failed to open pty")?;
        #[cfg(unix)]
        let shell_pid = pty.child().id();
        #[cfg(windows)]
        let shell_pid = pty.child_watcher().pid().map_or(0, |pid| pid.get());

        let event_loop = EventLoop::new(
            term.clone(),
            listener,
            pty,
            pty_options.drain_on_exit,
            false,
        )
        .context("failed to create event loop")?;
        let pty_tx = event_loop.channel();
        let _io_thread = event_loop.spawn();

        let terminal = Terminal {
            pty_tx: Notifier(pty_tx),
            term,
            events: Vec::new(),
            last_content: Content::default(),
            dirty,
            title: String::new(),
            shell_pid,
            _event_loop_task: Task::ready(()),
        };

        Ok(TerminalBuilder {
            terminal,
            events_rx,
        })
    }

    /// start pumping alacritty events into terminal entity
    pub fn subscribe(mut self, cx: &Context<Terminal>) -> Terminal {
        self.terminal._event_loop_task = cx.spawn(async move |terminal, cx| {
            while let Some(event) = self.events_rx.next().await {
                let Ok(()) = terminal.update(cx, |terminal, cx| {
                    // process the first event right away for lower latency
                    terminal.process_event(event, cx);
                }) else {
                    break;
                };

                // then batch rest in 4ms windows
                'outer: loop {
                    let mut events = Vec::new();
                    let mut timer = cx
                        .background_executor()
                        .timer(Duration::from_millis(4))
                        .fuse();
                    let mut wakeup = false;
                    loop {
                        futures::select_biased! {
                            _ = timer => break,
                            event = self.events_rx.next() => {
                                let Some(event) = event else { break };
                                if matches!(event, AlacTermEvent::Wakeup) {
                                    wakeup = true;
                                } else {
                                    events.push(event);
                                }
                                if events.len() > 100 {
                                    break;
                                }
                            },
                        }
                    }

                    if events.is_empty() && !wakeup {
                        break 'outer;
                    }

                    let Ok(()) = terminal.update(cx, |this, cx| {
                        if wakeup {
                            this.process_event(AlacTermEvent::Wakeup, cx);
                        }
                        for event in events {
                            this.process_event(event, cx);
                        }
                    }) else {
                        return;
                    };
                }
            }
        });
        self.terminal
    }
}
