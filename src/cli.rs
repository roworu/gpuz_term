//! command line options, they override config files and settings

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json_lenient::Value;

use crate::settings::{Commands, Keybindings, Pins, Settings, TabIcons, ThemeMode};
use crate::theme::Theme;

pub const USAGE: &str = "usage: kuterm [options]

options:
  --recreate-confs            delete config files and write defaults
  --config-file <path>        settings file to use instead of settings.jsonc
  --keybindings-file <path>   keybindings file to use instead of keybindings.jsonc
  --tab-icons-file <path>     tab icons file to use instead of tab_icons.jsonc
  --dark-theme-file <path>    dark theme file, overrides settings
  --light-theme-file <path>   light theme file, overrides settings
  --theme-mode <mode>         system, dark or light, overrides settings
  -h, --help                  print this help";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cli {
    pub help: bool,
    pub recreate_confs: bool,
    pub config_file: Option<PathBuf>,
    pub keybindings_file: Option<PathBuf>,
    pub tab_icons_file: Option<PathBuf>,
    pub dark_theme_file: Option<PathBuf>,
    pub light_theme_file: Option<PathBuf>,
    pub theme_mode: Option<ThemeMode>,
}

static CLI: OnceLock<Cli> = OnceLock::new();

impl Cli {
    /// parse arguments without the program name
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut cli = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let arg = arg.to_string_lossy().into_owned();
            let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
            match arg.as_str() {
                "-h" | "--help" => cli.help = true,
                "--recreate-confs" => cli.recreate_confs = true,
                "--config-file" => cli.config_file = Some(absolute(value()?)?),
                "--keybindings-file" => cli.keybindings_file = Some(absolute(value()?)?),
                "--tab-icons-file" => cli.tab_icons_file = Some(absolute(value()?)?),
                "--dark-theme-file" => cli.dark_theme_file = Some(absolute(value()?)?),
                "--light-theme-file" => cli.light_theme_file = Some(absolute(value()?)?),
                "--theme-mode" => {
                    let mode = value()?.to_string_lossy().into_owned();
                    cli.theme_mode = Some(
                        serde_json_lenient::from_value(Value::String(mode.clone()))
                            .map_err(|_| format!("unknown theme mode {mode:?}"))?,
                    );
                }
                _ => return Err(format!("unknown option {arg:?}")),
            }
        }
        Ok(cli)
    }

    /// store options for the whole process, only the first call wins
    pub fn init(self) {
        let _ = CLI.set(self);
    }

    /// options given at startup, defaults when not set
    pub fn get() -> &'static Self {
        CLI.get_or_init(Self::default)
    }

    /// apply overrides to loaded settings
    pub fn apply(&self, settings: &mut Settings) {
        let theme = &mut settings.theme;
        if let Some(mode) = self.theme_mode {
            theme.mode = mode;
        }
        if let Some(path) = &self.dark_theme_file {
            theme.dark = Some(path.clone());
        }
        if let Some(path) = &self.light_theme_file {
            theme.light = Some(path.clone());
        }
    }

    /// delete config files, so loading them writes defaults again
    pub fn remove_configs(&self) {
        // settings file is recreated too, so theme files come from defaults plus overrides
        let mut settings = Settings::default();
        self.apply(&mut settings);
        let themes = [settings.theme.dark, settings.theme.light]
            .into_iter()
            .flatten()
            .map(|path| Theme::resolve(&path));
        let files = [
            Settings::path(),
            Keybindings::path(),
            TabIcons::path(),
            Commands::path(),
            Pins::path(),
        ]
        .into_iter()
        .flatten()
        .chain(themes);
        for path in files {
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!("failed to remove {}: {error}", path.display());
            }
        }
    }
}

// relative paths start from the current folder, not from the settings folder
fn absolute(path: OsString) -> Result<PathBuf, String> {
    std::path::absolute(&path).map_err(|error| format!("bad path {path:?}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DEFAULT_SETTINGS;
    use crate::settings::tests::{temp_dir, with_config_home};

    fn parse(args: &[&str]) -> Result<Cli, String> {
        Cli::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_options() {
        assert_eq!(parse(&[]).unwrap(), Cli::default());
        let cli = parse(&[
            "--recreate-confs",
            "--config-file",
            "/a/s.jsonc",
            "--keybindings-file",
            "/a/k.jsonc",
            "--tab-icons-file",
            "/a/t.jsonc",
            "--dark-theme-file",
            "/a/d.jsonc",
            "--light-theme-file",
            "/a/l.jsonc",
            "--theme-mode",
            "light",
        ])
        .unwrap();
        assert_eq!(
            cli,
            Cli {
                help: false,
                recreate_confs: true,
                config_file: Some("/a/s.jsonc".into()),
                keybindings_file: Some("/a/k.jsonc".into()),
                tab_icons_file: Some("/a/t.jsonc".into()),
                dark_theme_file: Some("/a/d.jsonc".into()),
                light_theme_file: Some("/a/l.jsonc".into()),
                theme_mode: Some(ThemeMode::Light),
            }
        );
        assert!(parse(&["--help"]).unwrap().help);
    }

    #[test]
    fn relative_paths_start_from_current_folder() {
        let cli = parse(&["--dark-theme-file", "d.jsonc"]).unwrap();
        let expected = std::env::current_dir().unwrap().join("d.jsonc");
        assert_eq!(cli.dark_theme_file, Some(expected));
    }

    #[test]
    fn rejects_bad_options() {
        assert!(parse(&["--unknown"]).is_err());
        assert!(parse(&["--config-file"]).is_err());
        assert!(parse(&["--theme-mode", "blue"]).is_err());
    }

    #[test]
    fn apply_overrides_theme_settings() {
        let cli = parse(&["--theme-mode", "dark", "--light-theme-file", "/l.jsonc"]).unwrap();
        let mut settings = Settings::default();
        cli.apply(&mut settings);
        let defaults = Settings::default();
        assert_eq!(settings.theme.mode, ThemeMode::Dark);
        assert_eq!(settings.theme.dark, defaults.theme.dark);
        assert_eq!(settings.theme.light, Some("/l.jsonc".into()));

        let mut settings = Settings::default();
        Cli::default().apply(&mut settings);
        assert_eq!(settings, defaults);
    }

    #[test]
    fn remove_configs_lets_load_write_defaults() {
        let dir = temp_dir("cli_remove_configs");
        let custom_theme = dir.join("custom_dark.jsonc");
        with_config_home(&dir, || {
            let settings = Settings::path().unwrap();
            let keybindings = Keybindings::path().unwrap();
            std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
            std::fs::write(&settings, "broken").unwrap();
            std::fs::write(&keybindings, "broken").unwrap();
            std::fs::write(&custom_theme, "broken").unwrap();
            let unrelated = settings.with_file_name("notes.txt");
            std::fs::write(&unrelated, "keep").unwrap();

            let cli = Cli {
                recreate_confs: true,
                dark_theme_file: Some(custom_theme.clone()),
                ..Cli::default()
            };
            cli.remove_configs();
            assert!(!settings.exists());
            assert!(!keybindings.exists());
            assert!(!custom_theme.exists());
            assert!(unrelated.exists());

            assert_eq!(Settings::load(), Settings::default());
            assert_eq!(
                std::fs::read_to_string(&settings).unwrap(),
                DEFAULT_SETTINGS
            );
            assert_eq!(Keybindings::load(), Keybindings::default());
            assert!(keybindings.exists());

            // nothing to remove is fine
            cli.remove_configs();
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
