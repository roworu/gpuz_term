use std::time::{Duration, Instant};

use gpui::point;

use alacritty_terminal::event::Event as AlacTermEvent;
use futures::{FutureExt, StreamExt};
use gpui::{Bounds, px, size};

use super::{Terminal, TerminalBounds, TerminalBuilder};
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
        assert!(Instant::now() < deadline, "no {needle:?} on screen:\n{text}");
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
