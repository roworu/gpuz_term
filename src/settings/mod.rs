//! user settings, read from a jsonc file

mod keybindings;
mod options;
mod tab_icons;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::{App, Global};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json_lenient::Value;

pub use keybindings::Keybindings;
pub use options::{
    CursorShape, LineHeight, NewTabButton, Shell, TabIconPosition, TabTitleAlign, TabTitleBlock,
    ThemeMode,
};
pub use tab_icons::TabIcons;

use crate::cli::Cli;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Settings {
    pub ui_font_family: String,
    pub ui_font_size: f32,
    pub hide_bar_for_one_tab: bool,
    pub expand_tabs: bool,
    pub tab_width: u32,
    pub tab_title: Vec<TabTitleBlock>,
    pub tab_title_align: TabTitleAlign,
    pub tab_icon: TabIconSettings,
    pub new_tab_button: NewTabButton,
    pub window_title: Vec<TabTitleBlock>,
    pub default_title: String,
    pub theme: ThemeSettings,
    pub terminal: TerminalSettings,
    pub profiles: Vec<Profile>,
}

impl Default for Settings {
    fn default() -> Self {
        serde_json_lenient::from_str(DEFAULT_SETTINGS)
            .expect("bundled default settings are invalid")
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
pub struct TabIconSettings {
    pub position: TabIconPosition,
    pub dynamic: bool,
    pub default: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TerminalSettings {
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

/// what a new tab starts with
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Profile {
    pub name: String,
    /// opened by the new tab action and a left click on "+"
    #[serde(default)]
    pub default: bool,
    pub command: Shell,
    /// folder to start in, kuterm's own folder when none
    pub working_directory: Option<PathBuf>,
    /// color scheme of the terminal, global theme when none
    pub theme: Option<ThemeSettings>,
    pub icon: Option<String>,
    /// extra environment variables for the command
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Global for Settings {}

// smaller fonts break the terminal grid, bigger ones stop growing
const FONT_SIZE_RANGE: (f32, f32) = (6., 72.);
// lines below 1 overlap, above 3 waste the screen
const LINE_HEIGHT_RANGE: (f32, f32) = (1., 3.);

fn limit(name: &str, value: f32, (min, max): (f32, f32)) -> Option<f32> {
    if value.is_nan() || value < min {
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

/// read a config file, creating it from `defaults` first; falls back to defaults when missing or invalid
pub(crate) fn load_file<T: Default>(
    path: Option<PathBuf>,
    defaults: &str,
    what: &str,
    parse: impl Fn(&str) -> serde_json_lenient::Result<T>,
) -> T {
    let Some(path) = path else {
        return T::default();
    };
    create_default_file(&path, defaults);
    let Ok(json) = std::fs::read_to_string(&path) else {
        return T::default();
    };
    parse(&json).unwrap_or_else(|error| {
        eprintln!("invalid {what} in {}: {error}", path.display());
        T::default()
    })
}

/// `$XDG_CONFIG_HOME/kuterm`, falling back to `~/.config/kuterm`
pub fn config_dir() -> Option<PathBuf> {
    // xdg says empty or relative values must be ignored
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config_dir.join("kuterm"))
}

impl Settings {
    /// `--config-file`, or `settings.jsonc` in the config dir
    pub fn path() -> Option<PathBuf> {
        Cli::get()
            .config_file
            .clone()
            .or_else(|| Some(config_dir()?.join("settings.jsonc")))
    }

    /// parse settings, we allow comments and trailing commas
    pub fn parse(json: &str) -> serde_json_lenient::Result<Self> {
        let mut settings: Self = parse_over(DEFAULT_SETTINGS, json)?;
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
        if settings.profiles.is_empty() {
            eprintln!("profiles is empty, using the default profiles");
            settings.profiles = defaults.profiles;
        }
        // exactly one profile is the default, the first marked one wins
        match settings.profiles.iter().position(|profile| profile.default) {
            Some(first) => {
                let (head, rest) = settings.profiles.split_at_mut(first + 1);
                for profile in rest.iter_mut().filter(|profile| profile.default) {
                    eprintln!(
                        "profile {:?} is also default, using {:?}",
                        profile.name, head[first].name
                    );
                    profile.default = false;
                }
            }
            None => settings.profiles[0].default = true,
        }
        Ok(settings)
    }

    /// profile opened by the new tab action
    pub fn default_profile(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|profile| profile.default)
            .unwrap_or(&self.profiles[0])
    }

    /// load settings from the settings file, using defaults when it is missing or invalid
    pub fn load() -> Self {
        load_file(Self::path(), DEFAULT_SETTINGS, "settings", Self::parse)
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

/// parse `json` deep merged over the bundled `defaults`
pub(crate) fn parse_over<T: DeserializeOwned>(
    defaults: &str,
    json: &str,
) -> serde_json_lenient::Result<T> {
    let mut value: Value = serde_json_lenient::from_str(defaults)?;
    merge(&mut value, serde_json_lenient::from_str(json)?);
    serde_json_lenient::from_value(value)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;

    use super::*;

    // env vars are process wide, so tests that set XDG_CONFIG_HOME take this lock
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// fresh temp dir unique to this test process
    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kuterm_{name}_{}", std::process::id()));
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
    }

    #[test]
    fn bundled_settings_are_commented_and_parse() {
        assert!(DEFAULT_SETTINGS.contains("//"));
        assert_eq!(
            Settings::parse(DEFAULT_SETTINGS).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn parse_settings() {
        let settings = Settings::parse(
            r#"{
                // comments are allowed
                "ui_font_family": "JetBrainsMonoNL Nerd Font Mono",
                "ui_font_size": 16, // inline comments too
                "profiles": [{
                    "name": "bash",
                    "command": {
                        "with_arguments": { "program": "/bin/bash", "args": ["--login"] }
                    },
                }],
                "terminal": {
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
            settings.default_profile().command,
            Shell::WithArguments {
                program: "/bin/bash".into(),
                args: vec!["--login".into()],
            }
        );
        assert_eq!(
            settings.terminal.font_family,
            "JetBrainsMonoNL Nerd Font Mono"
        );
        assert_eq!(settings.terminal.font_size, 16.0);
        assert_eq!(settings.terminal.line_height.value(), 2.);
        assert_eq!(settings.terminal.cursor_shape, CursorShape::Bar);
    }

    #[test]
    fn parses_shell_variants() {
        let parse = |command: &str| {
            let json = format!(r#"{{"profiles": [{{"name": "a", "command": {command}}}]}}"#);
            Settings::parse(&json).unwrap().profiles[0].command.clone()
        };
        assert_eq!(parse(r#""system""#), Shell::System);
        assert_eq!(parse(r#"{"program": "zsh"}"#), Shell::Program("zsh".into()));
    }

    #[test]
    fn partial_terminal_section_keeps_other_defaults() {
        let settings = Settings::parse(r#"{"terminal": {"cursor_shape": "hollow"}}"#).unwrap();
        let defaults = Settings::default();
        assert_eq!(settings.terminal.cursor_shape, CursorShape::Hollow);
        assert_eq!(settings.terminal.font_size, defaults.terminal.font_size);
        assert_eq!(settings.ui_font_size, defaults.ui_font_size);
    }

    #[test]
    fn partial_top_level_section_keeps_terminal_defaults() {
        let settings = Settings::parse(r#"{"ui_font_size": 12}"#).unwrap();
        let expected = Settings {
            ui_font_size: 12.,
            ..Settings::default()
        };
        assert_eq!(settings, expected);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let settings =
            Settings::parse(r#"{"foo": 1, "terminal": {"bar": true, "font_size": 14}}"#).unwrap();
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
        assert_eq!(
            align(r#"{"tab_title_align": "center"}"#),
            TabTitleAlign::Center
        );
        assert_eq!(
            align(r#"{"tab_title_align": "right"}"#),
            TabTitleAlign::Right
        );
        assert!(Settings::parse(r#"{"tab_title_align": "middle"}"#).is_err());
        assert!(Settings::parse(r#"{"tab_title": "number"}"#).is_err());
    }

    #[test]
    fn theme_paths_keep_other_defaults() {
        let settings = Settings::parse(r#"{"theme": {"dark": "themes/d.jsonc"}}"#).unwrap();
        assert_eq!(settings.theme.mode, ThemeMode::System);
        assert_eq!(settings.theme.dark, Some(PathBuf::from("themes/d.jsonc")));
        assert_eq!(settings.theme.light, Settings::default().theme.light);

        let settings = Settings::parse(
            r#"{"theme": {"mode": "dark", "dark": null, "light": "/abs/l.jsonc"}}"#,
        )
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
            r#"{"profiles": [{"name": "a", "command": {"unknown": "zsh"}}]}"#,
            r#"{"profiles": [{"name": "a", "command": {"with_arguments": {"program": "bash"}}}]}"#,
            r#"{"profiles": [{"name": "a"}]}"#,
            r#"{"profiles": [{"command": "system"}]}"#,
            r#"{"profiles": [{"name": "a", "command": "system", "env": {"A": 1}}]}"#,
            r#"{"profiles": [{"name": "a", "command": "system", "theme": "dark"}]}"#,
            r#"{"profiles": {"name": "a", "command": "system"}}"#,
            r#"{"theme": "dark"}"#,
            r#"{"theme": {"mode": "auto"}}"#,
            r#"{"theme": {"mode": null}}"#,
            r#"{"theme": {"dark": 5}}"#,
        ] {
            assert!(
                Settings::parse(json).is_err(),
                "expected error for {json:?}"
            );
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
            let path = dir.join("kuterm").join("settings.jsonc");
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

    fn font_json(field: &str, value: &str) -> String {
        match field {
            "ui_font_size" => format!(r#"{{"ui_font_size": {value}}}"#),
            _ => format!(r#"{{"terminal": {{"font_size": {value}}}}}"#),
        }
    }

    fn parse_font(field: &str, value: &str) -> f32 {
        let settings = Settings::parse(&font_json(field, value)).unwrap();
        match field {
            "ui_font_size" => settings.ui_font_size,
            _ => settings.terminal.font_size,
        }
    }

    #[test]
    fn font_size_below_min_uses_default() {
        let defaults = Settings::default();
        for field in ["ui_font_size", "terminal.font_size"] {
            for value in [
                "0", "0.0", "-0", "-0.0", "1", "0.5", "1e-30", "5", "-1", "-6", "-16", "-72",
                "-100", "-1e30", "-3.4e38",
            ] {
                let got = parse_font(field, value);
                let default = match field {
                    "ui_font_size" => defaults.ui_font_size,
                    _ => defaults.terminal.font_size,
                };
                // exactly the default, not -0.0 or a clamp to 6
                assert_eq!(
                    got.to_bits(),
                    default.to_bits(),
                    "{field} {value} gave {got}"
                );
            }
        }
    }

    #[test]
    fn font_size_above_max_is_capped() {
        for field in ["ui_font_size", "terminal.font_size"] {
            for value in [
                "72.5", "73", "80", "100", "200", "1000", "1e6", "1e30", "3.4e38", "1e39", "1e300",
            ] {
                let got = parse_font(field, value);
                assert_eq!(got, 72.0, "{field} {value} gave {got}");
            }
        }
    }

    #[test]
    fn line_height_custom_boundaries() {
        let lh = |v: &str| {
            let json = format!(r#"{{"terminal": {{"line_height": {{"custom": {v}}}}}}}"#);
            Settings::parse(&json).unwrap().terminal.line_height
        };
        assert_eq!(lh("0.99"), LineHeight::Standard);
        assert_eq!(lh("0.9999"), LineHeight::Standard);
        assert_eq!(lh("1"), LineHeight::Custom(1.0));
        assert_eq!(lh("3"), LineHeight::Custom(3.0));
        assert_eq!(lh("3.01"), LineHeight::Custom(3.0));
        assert_eq!(lh("3.0001"), LineHeight::Custom(3.0));
    }

    #[test]
    fn tab_title_array_replaces_default_whole() {
        // default has 3 blocks, a shorter or longer user array must not be index merged
        let settings = Settings::parse(r#"{"tab_title": ["number", "prompt"]}"#).unwrap();
        assert_eq!(
            settings.tab_title,
            vec![TabTitleBlock::Number, TabTitleBlock::Prompt]
        );
        let settings = Settings::parse(r#"{"tab_title": ["folder"]}"#).unwrap();
        assert_eq!(settings.tab_title, vec![TabTitleBlock::Folder]);
    }

    #[test]
    fn bundled_settings_have_one_default_profile() {
        let settings = Settings::default();
        assert_eq!(settings.profiles.len(), 1);
        let profile = settings.default_profile();
        assert!(profile.default);
        assert_eq!(profile.command, Shell::System);
        assert_eq!(profile.working_directory, None);
        assert_eq!(profile.theme, None);
        assert!(profile.env.is_empty());
    }

    #[test]
    fn profile_optional_keys_can_be_left_out() {
        let settings = Settings::parse(
            r#"{"profiles": [
                {"name": "a", "command": "system"},
                {
                    "name": "b",
                    "default": true,
                    "command": {"program": "zsh"},
                    "working_directory": "~/src",
                    "theme": {"mode": "dark", "dark": "themes/b.jsonc"},
                    "env": {"EDITOR": "vim"},
                },
            ]}"#,
        )
        .unwrap();
        let [a, b] = &settings.profiles[..] else {
            panic!("expected two profiles");
        };
        assert!(!a.default);
        assert_eq!(a.working_directory, None);
        assert_eq!(a.theme, None);
        assert!(a.env.is_empty());
        assert_eq!(settings.default_profile(), b);
        assert_eq!(b.command, Shell::Program("zsh".into()));
        assert_eq!(b.working_directory, Some(PathBuf::from("~/src")));
        let theme = b.theme.as_ref().unwrap();
        assert_eq!(theme.mode, ThemeMode::Dark);
        assert_eq!(theme.dark, Some(PathBuf::from("themes/b.jsonc")));
        assert_eq!(theme.light, None);
        assert_eq!(b.env["EDITOR"], "vim");
    }

    #[test]
    fn exactly_one_profile_is_default() {
        let a = r#"{"name": "a", "command": "system"}"#;
        let a_default = r#"{"name": "a", "default": true, "command": "system"}"#;
        let b = r#"{"name": "b", "command": "system"}"#;
        let b_default = r#"{"name": "b", "default": true, "command": "system"}"#;
        let c_default = r#"{"name": "c", "default": true, "command": "system"}"#;
        // default flags of the parsed profiles, in order
        let parse = |profiles: &[&str]| -> Vec<bool> {
            let json = format!(r#"{{"profiles": [{}]}}"#, profiles.join(","));
            let settings = Settings::parse(&json).unwrap();
            settings
                .profiles
                .iter()
                .map(|profile| profile.default)
                .collect()
        };
        // none marked, the first one is used
        assert_eq!(parse(&[a, b]), [true, false]);
        assert_eq!(parse(&[b_default]), [true]);
        assert_eq!(parse(&[a, b_default]), [false, true]);
        // several marked, the first marked one wins
        assert_eq!(parse(&[a, b_default, c_default]), [false, true, false]);
        assert_eq!(
            parse(&[a_default, b_default, c_default]),
            [true, false, false]
        );
    }

    #[test]
    fn parses_tab_icon() {
        let defaults = Settings::default().tab_icon;
        let parse = |json: &str| Settings::parse(json).unwrap().tab_icon;
        for (json, position) in [
            (
                r#"{"tab_icon": {"position": "left"}}"#,
                TabIconPosition::Left,
            ),
            (
                r#"{"tab_icon": {"position": "right"}}"#,
                TabIconPosition::Right,
            ),
        ] {
            let expected = TabIconSettings {
                position,
                ..defaults.clone()
            };
            assert_eq!(parse(json), expected);
        }
        for dynamic in [true, false] {
            let json = format!(r#"{{"tab_icon": {{"dynamic": {dynamic}, "default": "D"}}}}"#);
            let icon = parse(&json);
            assert_eq!(icon.dynamic, dynamic);
            assert_eq!(icon.default, "D");
            assert_eq!(icon.position, defaults.position);
        }

        for json in [
            r#"{"tab_icon": "left"}"#,
            r#"{"tab_icon": {"position": "top"}}"#,
            r#"{"tab_icon": {"dynamic": "yes"}}"#,
            r#"{"tab_icon": {"default": null}}"#,
            r#"{"profiles": [{"name": "a", "command": "system", "icon": 5}]}"#,
        ] {
            assert!(
                Settings::parse(json).is_err(),
                "expected error for {json:?}"
            );
        }
    }

    #[test]
    fn profile_icon_is_optional() {
        let settings = Settings::parse(
            r#"{"profiles": [
                {"name": "a", "command": "system"},
                {"name": "b", "command": "system", "icon": "B"},
                {"name": "c", "command": "system", "icon": null},
            ]}"#,
        )
        .unwrap();
        let icons: Vec<_> = settings
            .profiles
            .iter()
            .map(|p| p.icon.as_deref())
            .collect();
        assert_eq!(icons, [None, Some("B"), None]);
    }

    #[test]
    fn empty_profiles_use_bundled_ones() {
        assert_eq!(
            Settings::parse(r#"{"profiles": []}"#).unwrap().profiles,
            Settings::default().profiles
        );
    }
}
