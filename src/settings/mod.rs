//! user settings, read from a json file

mod options;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use gpui::{App, Global};
use serde::Deserialize;

pub use options::{CursorShape, LineHeight, Shell};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub ui_font_family: String,
    pub ui_font_size: f32,
    pub terminal: TerminalSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_font_family: ".SystemUIFont".into(),
            ui_font_size: 16.,
            terminal: TerminalSettings::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct TerminalSettings {
    pub shell: Shell,
    pub font_family: String,
    pub font_size: f32,
    pub line_height: LineHeight,
    pub cursor_shape: CursorShape,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: Shell::System,
            font_family: "JetBrainsMonoNL Nerd Font Mono".into(),
            font_size: 16.,
            line_height: LineHeight::Standard,
            cursor_shape: CursorShape::Block,
        }
    }
}

impl Global for Settings {}

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
        serde_json_lenient::from_str(json)
    }

    /// load settings from the settings file, using defaults when it is missing or invalid
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
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
