//! user settings, read from a json file

mod options;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use gpui::{App, Global};
use serde::Deserialize;
use serde_json_lenient::Value;

pub use options::{CursorShape, LineHeight, Shell, ThemeMode};

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Settings {
    pub ui_font_family: String,
    pub ui_font_size: f32,
    pub hide_bar_for_one_tab: bool,
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

/// commented settings file written on first launch
pub const DEFAULT_SETTINGS: &str = include_str!("../../assets/default_settings.json");

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
    /// `$XDG_CONFIG_HOME/gpuz_term/settings.json`, falling back to `~/.config`
    pub fn path() -> Option<PathBuf> {
        // xdg says empty or relative values must be ignored
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(config_dir.join("gpuz_term").join("settings.json"))
    }

    /// parse settings, we allow comments and trailing commas
    pub fn parse(json: &str) -> serde_json_lenient::Result<Self> {
        let mut settings: Value = serde_json_lenient::from_str(DEFAULT_SETTINGS)?;
        merge(&mut settings, serde_json_lenient::from_str(json)?);
        serde_json_lenient::from_value(settings)
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
