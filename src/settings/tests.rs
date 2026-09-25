//! parsing tests for the settings file

use super::{CursorShape, Settings, Shell};

#[test]
fn empty_file_uses_zed_defaults() {
    let settings = Settings::parse("{}").unwrap();
    assert_eq!(settings, Settings::default());
    assert_eq!(settings.ui_font_size, 16.);
    assert_eq!(settings.terminal.font_size, 16.);
    assert_eq!(settings.terminal.font_family, "JetBrainsMonoNL Nerd Font Mono");
    assert_eq!(settings.terminal.shell, Shell::System);
    assert_eq!(settings.terminal.line_height.value(), 1.3);
    assert_eq!(settings.terminal.cursor_shape, CursorShape::Block);
}

#[test]
fn parses_zed_style_settings() {
    let settings = Settings::parse(
        r#"{
            // comments and trailing commas are allowed, like in zed
            "ui_font_family": "JetBrainsMonoNL Nerd Font Mono",
            "ui_font_size": 16,
            "terminal": {
                "shell": {
                    "with_arguments": { "program": "/bin/bash", "args": ["--login"] }
                },
                "font_family": "JetBrainsMonoNL Nerd Font Mono",
                "font_size": 16,
                "line_height": { "custom": 2 },
                "cursor_shape": "bar",
            },
        }"#,
    )
    .unwrap();
    assert_eq!(settings.ui_font_family, "JetBrainsMonoNL Nerd Font Mono");
    assert_eq!(settings.ui_font_size, 16.);
    assert_eq!(
        settings.terminal.shell,
        Shell::WithArguments {
            program: "/bin/bash".into(),
            args: vec!["--login".into()],
        }
    );
    assert_eq!(settings.terminal.font_family, "JetBrainsMonoNL Nerd Font Mono");
    assert_eq!(settings.terminal.font_size, 16.0);
    assert_eq!(settings.terminal.line_height.value(), 2.);
    assert_eq!(settings.terminal.cursor_shape, CursorShape::Bar);
}

#[test]
fn parses_shell_variants() {
    let parse = |json: &str| Settings::parse(json).unwrap().terminal.shell;
    assert_eq!(parse(r#"{"terminal": {"shell": "system"}}"#), Shell::System);
    assert_eq!(
        parse(r#"{"terminal": {"shell": {"program": "zsh"}}}"#),
        Shell::Program("zsh".into())
    );
}

#[test]
fn partial_terminal_section_keeps_other_defaults() {
    let settings = Settings::parse(r#"{"terminal": {"cursor_shape": "hollow"}}"#).unwrap();
    assert_eq!(settings.terminal.cursor_shape, CursorShape::Hollow);
    assert_eq!(settings.terminal.font_size, 16.);
    assert_eq!(settings.ui_font_size, 16.);
}

#[test]
fn rejects_unknown_values() {
    assert!(Settings::parse(r#"{"terminal": {"cursor_shape": "triangle"}}"#).is_err());
}
