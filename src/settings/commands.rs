//! command palette commands, read from a jsonc file next to settings

use std::path::PathBuf;

use gpui::{App, Global};
use serde::Deserialize;

use super::{config_dir, load_file, parse_over};

/// commented commands file written on first launch
pub const DEFAULT_COMMANDS: &str = include_str!("../../assets/default_commands.jsonc");

/// one step of a command
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CommandAction {
    About,
    ReloadSettings,
    ReloadThemes,
    ReloadKeybindings,
    ReloadAll,
    NewTab,
    CloseTab,
    NextTab,
    /// text written to the active tab as if typed
    Type(String),
}

/// palette entry, its actions run in order
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Command {
    pub name: String,
    pub actions: Vec<CommandAction>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Commands {
    pub commands: Vec<Command>,
}

impl Default for Commands {
    fn default() -> Self {
        Self::parse("{}").expect("bundled commands are invalid")
    }
}

impl Global for Commands {}

impl Commands {
    /// `commands.jsonc` in the config dir
    pub fn path() -> Option<PathBuf> {
        Some(config_dir()?.join("commands.jsonc"))
    }

    /// parse commands over the bundled ones, a user list replaces the bundled one whole
    pub fn parse(json: &str) -> serde_json_lenient::Result<Self> {
        parse_over(DEFAULT_COMMANDS, json)
    }

    /// load commands from their file, using defaults when it is missing or invalid
    pub fn load() -> Self {
        load_file(Self::path(), DEFAULT_COMMANDS, "commands", Self::parse)
    }

    /// commands loaded at startup or on the last reload
    pub fn get(cx: &App) -> &Self {
        cx.global::<Self>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::tests::{temp_dir, with_config_home};

    #[test]
    fn bundled_commands_are_commented_and_parse() {
        assert!(DEFAULT_COMMANDS.contains("//"));
        assert_eq!(
            Commands::parse(DEFAULT_COMMANDS).unwrap(),
            Commands::default()
        );
        assert!(!Commands::default().commands.is_empty());
    }

    #[test]
    fn parses_every_action() {
        let commands = Commands::parse(
            r#"{"commands": [{"name": "all", "actions": [
                "about", "reload_settings", "reload_themes", "reload_keybindings",
                "reload_all", "new_tab", "close_tab", "next_tab", {"type": "ls\n"},
            ]}]}"#,
        )
        .unwrap();
        assert_eq!(
            commands.commands,
            vec![Command {
                name: "all".into(),
                actions: vec![
                    CommandAction::About,
                    CommandAction::ReloadSettings,
                    CommandAction::ReloadThemes,
                    CommandAction::ReloadKeybindings,
                    CommandAction::ReloadAll,
                    CommandAction::NewTab,
                    CommandAction::CloseTab,
                    CommandAction::NextTab,
                    CommandAction::Type("ls\n".into()),
                ],
            }]
        );
    }

    #[test]
    fn invalid_commands_are_rejected() {
        for json in [
            r#"{"commands": [{"name": "x", "actions": ["launch_rockets"]}]}"#,
            r#"{"commands": [{"actions": ["about"]}]}"#,
            r#"{"commands": [{"name": "x"}]}"#,
            r#"{"commands": [{"name": "x", "actions": [{"type": 5}]}]}"#,
            r#"{"commands": "about"}"#,
        ] {
            assert!(Commands::parse(json).is_err(), "{json} should be invalid");
        }
    }

    #[test]
    fn load_creates_keeps_and_falls_back() {
        let dir = temp_dir("commands_load");
        with_config_home(&dir, || {
            let path = Commands::path().unwrap();
            assert_eq!(path, dir.join("kuterm/commands.jsonc"));
            assert_eq!(Commands::load(), Commands::default());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT_COMMANDS);

            let json = r#"{"commands": [{"name": "only", "actions": ["new_tab"]}]}"#;
            std::fs::write(&path, json).unwrap();
            assert_eq!(Commands::load().commands.len(), 1);

            std::fs::write(&path, "{ broken").unwrap();
            assert_eq!(Commands::load(), Commands::default());
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
