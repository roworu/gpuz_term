//! user settings, read from a jsonc file

mod keybindings;
mod options;

use std::path::{Path, PathBuf};

use gpui::{App, Global};
use serde::Deserialize;
use serde_json_lenient::Value;

pub use keybindings::Keybindings;
pub use options::{CursorShape, LineHeight, Shell, TabTitleAlign, TabTitleBlock, ThemeMode};

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Settings {
    pub ui_font_family: String,
    pub ui_font_size: f32,
    pub hide_bar_for_one_tab: bool,
    pub expand_tabs: bool,
    pub tab_width: u32,
    pub tab_title: Vec<TabTitleBlock>,
    pub tab_title_align: TabTitleAlign,
    pub window_title: Vec<TabTitleBlock>,
    pub default_title: String,
    pub theme: ThemeSettings,
    pub terminal: TerminalSettings,
}

impl Default for Settings {
    fn default() -> Self {
        serde_json_lenient::from_str(DEFAULT_SETTINGS).expect("bundled default settings are invalid")
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ThemeSettings {
    pub mode: ThemeMode,
    /// custom dark theme file, bundled one when none
    pub dark: Option<PathBuf>,
    /// custom light theme file, bundled one when none
    pub light: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TerminalSettings {
    pub shell: Shell,
    pub font_family: String,
    pub font_size: f32,
    pub line_height: LineHeight,
    pub cursor_shape: CursorShape,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Settings::default().terminal
    }
}

impl Global for Settings {}

// smaller fonts break the terminal grid, bigger ones stop growing
const FONT_SIZE_RANGE: (f32, f32) = (6., 72.);
// lines below 1 overlap, above 3 waste the screen
const LINE_HEIGHT_RANGE: (f32, f32) = (1., 3.);

fn limit(name: &str, value: f32, (min, max): (f32, f32)) -> Option<f32> {
    if !(value >= min) {
        eprintln!("{name} {value} is below {min}, using the default");
        return None;
    }
    if value > max {
        eprintln!("{name} {value} is above {max}, using {max}");
    }
    Some(value.min(max))
}

/// commented settings file written on first launch
pub const DEFAULT_SETTINGS: &str = include_str!("../../assets/default_settings.jsonc");

/// write a default config file if it does not exist yet, so users can see what to change
pub fn create_default_file(path: &Path, contents: &str) {
    if path.exists() {
        return;
    }
    let result = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, contents));
    if let Err(error) = result {
        eprintln!("failed to create {}: {error}", path.display());
    }
}

impl Settings {
    /// `$XDG_CONFIG_HOME/gpuz_term/settings.jsonc`, falling back to `~/.config`
    pub fn path() -> Option<PathBuf> {
        // xdg says empty or relative values must be ignored
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(config_dir.join("gpuz_term").join("settings.jsonc"))
    }

    /// parse settings, we allow comments and trailing commas
    pub fn parse(json: &str) -> serde_json_lenient::Result<Self> {
        let mut settings: Value = serde_json_lenient::from_str(DEFAULT_SETTINGS)?;
        merge(&mut settings, serde_json_lenient::from_str(json)?);
        let mut settings: Self = serde_json_lenient::from_value(settings)?;
        let defaults = Self::default();
        settings.ui_font_size = limit("ui_font_size", settings.ui_font_size, FONT_SIZE_RANGE)
            .unwrap_or(defaults.ui_font_size);
        let terminal = &mut settings.terminal;
        terminal.font_size = limit("terminal.font_size", terminal.font_size, FONT_SIZE_RANGE)
            .unwrap_or(defaults.terminal.font_size);
        if let LineHeight::Custom(value) = terminal.line_height {
            terminal.line_height = limit("terminal.line_height", value, LINE_HEIGHT_RANGE)
                .map_or(defaults.terminal.line_height, LineHeight::Custom);
        }
        Ok(settings)
    }

    /// load settings from the settings file, using defaults when it is missing or invalid
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        create_default_file(&path, DEFAULT_SETTINGS);
        let Ok(json) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        Self::parse(&json).unwrap_or_else(|error| {
            eprintln!("invalid settings in {}: {error}", path.display());
            Self::default()
        })
    }

    /// settings loaded at startup
    pub fn get(cx: &App) -> &Self {
        cx.global::<Self>()
    }
}

// user values override defaults key by key, so a partial file keeps the other defaults.
// anything that is not an object on both sides (enum values, arrays) is replaced as a whole
pub(crate) fn merge(base: &mut Value, overrides: Value) {
    match (base, overrides) {
        (Value::Object(base), Value::Object(overrides)) => {
            for (key, value) in overrides {
                // a missing key starts as null, which the fallback arm replaces with the value
                merge(base.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, overrides) => *base = overrides,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;

    use super::*;

    // env vars are process wide, so tests that set XDG_CONFIG_HOME take this lock
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// fresh temp dir unique to this test process
    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gpuz_term_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// run `f` with XDG_CONFIG_HOME pointing at `dir`, restoring it afterwards
    pub(crate) fn with_config_home(dir: &Path, f: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::set_var("XDG_CONFIG_HOME", dir) };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        unsafe {
            match old {
                Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn empty_file_uses_defaults() {
        let settings = Settings::parse("{}").unwrap();
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.ui_font_size, 16.);
        assert_eq!(settings.terminal.font_size, 16.);
        assert_eq!(settings.terminal.font_family, "JetBrainsMonoNL Nerd Font Mono");
        assert_eq!(settings.terminal.shell, Shell::System);
        assert_eq!(settings.terminal.line_height.value(), 1.3);
        assert_eq!(settings.terminal.cursor_shape, CursorShape::Bar);
    }

    #[test]
    fn bundled_settings_are_commented_and_parse() {
        assert!(DEFAULT_SETTINGS.contains("//"));
        assert_eq!(Settings::parse(DEFAULT_SETTINGS).unwrap(), Settings::default());
    }

    #[test]
    fn parse_settings() {
        let settings = Settings::parse(
            r#"{
                // comments are allowed
                "ui_font_family": "JetBrainsMonoNL Nerd Font Mono",
                "ui_font_size": 16, // inline comments too
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
    fn partial_top_level_section_keeps_terminal_defaults() {
        let settings = Settings::parse(r#"{"ui_font_size": 12}"#).unwrap();
        let mut expected = Settings::default();
        expected.ui_font_size = 12.;
        assert_eq!(settings, expected);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let settings = Settings::parse(r#"{"foo": 1, "terminal": {"bar": true, "font_size": 14}}"#).unwrap();
        let mut expected = Settings::default();
        expected.terminal.font_size = 14.;
        assert_eq!(settings, expected);
    }

    #[test]
    fn parses_tab_title_blocks() {
        let settings = Settings::parse(
            r#"{"tab_title": ["number", {"text": ": "}, "prompt", "folder", "command", "title", {"exec": "date"}]}"#,
        )
        .unwrap();
        assert_eq!(
            settings.tab_title,
            vec![
                TabTitleBlock::Number,
                TabTitleBlock::Text(": ".into()),
                TabTitleBlock::Prompt,
                TabTitleBlock::Folder,
                TabTitleBlock::Command,
                TabTitleBlock::Title,
                TabTitleBlock::Exec("date".into()),
            ]
        );
        assert!(Settings::parse(r#"{"tab_title": ["unknown"]}"#).is_err());
        assert_eq!(Settings::default().tab_title_align, TabTitleAlign::Left);
        let align = |json: &str| Settings::parse(json).unwrap().tab_title_align;
        assert_eq!(align(r#"{"tab_title_align": "center"}"#), TabTitleAlign::Center);
        assert_eq!(align(r#"{"tab_title_align": "right"}"#), TabTitleAlign::Right);
        assert!(Settings::parse(r#"{"tab_title_align": "middle"}"#).is_err());
        assert!(Settings::parse(r#"{"tab_title": "number"}"#).is_err());
    }

    #[test]
    fn theme_paths_keep_other_defaults() {
        let settings = Settings::parse(r#"{"theme": {"dark": "themes/d.jsonc"}}"#).unwrap();
        assert_eq!(settings.theme.mode, ThemeMode::System);
        assert_eq!(settings.theme.dark, Some(PathBuf::from("themes/d.jsonc")));
        assert_eq!(settings.theme.light, Settings::default().theme.light);

        let settings =
            Settings::parse(r#"{"theme": {"mode": "dark", "dark": null, "light": "/abs/l.jsonc"}}"#)
                .unwrap();
        assert_eq!(settings.theme.mode, ThemeMode::Dark);
        assert_eq!(settings.theme.dark, None);
        assert_eq!(settings.theme.light, Some(PathBuf::from("/abs/l.jsonc")));
        assert_eq!(settings.terminal, Settings::default().terminal);
    }

    #[test]
    fn rejects_unknown_values() {
        for json in [
            "",
            "{",
            "not json",
            "[]",
            "5",
            "null",
            r#"{"terminal": 5}"#,
            r#"{"tab_width": -1}"#,
            r#"{"tab_width": 60.5}"#,
            r#"{"ui_font_size": "big"}"#,
            r#"{"terminal": {"cursor_shape": "triangle"}}"#,
            r#"{"terminal": {"line_height": "tall"}}"#,
            r#"{"terminal": {"shell": {"unknown": "zsh"}}}"#,
            r#"{"terminal": {"shell": {"with_arguments": {"program": "bash"}}}}"#,
            r#"{"theme": "dark"}"#,
            r#"{"theme": {"mode": "auto"}}"#,
            r#"{"theme": {"mode": null}}"#,
            r#"{"theme": {"dark": 5}}"#,
        ] {
            assert!(Settings::parse(json).is_err(), "expected error for {json:?}");
        }
    }

    #[test]
    fn create_default_file_creates_dirs_and_never_overwrites() {
        let dir = temp_dir("create_default");
        let path = dir.join("a").join("b").join("file.jsonc");
        create_default_file(&path, "first");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
        create_default_file(&path, "second");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_creates_and_reads_settings_file() {
        let dir = temp_dir("settings_load");
        with_config_home(&dir, || {
            let path = dir.join("gpuz_term").join("settings.jsonc");
            assert_eq!(Settings::path(), Some(path.clone()));

            // missing file is created from the bundled one
            assert_eq!(Settings::load(), Settings::default());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT_SETTINGS);

            std::fs::write(&path, r#"{"theme": {"mode": "light", "light": "x.jsonc"}}"#).unwrap();
            let settings = Settings::load();
            assert_eq!(settings.theme.mode, ThemeMode::Light);
            assert_eq!(settings.theme.light, Some(PathBuf::from("x.jsonc")));

            // broken file falls back to defaults and is left untouched
            let broken = r#"{"theme": {"mode": "auto"}}"#;
            std::fs::write(&path, broken).unwrap();
            assert_eq!(Settings::load(), Settings::default());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
