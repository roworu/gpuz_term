//! theme colors, dark or light from bundled or custom jsonc files

use std::path::{Path, PathBuf};

use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::{App, Global, Hsla, WindowAppearance, rgb};
use serde::Deserialize;
use serde_json_lenient::Value;

use crate::settings::{Settings, ThemeMode, ThemeSettings, create_default_file, merge};

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Theme {
    pub tab_bar_background: Hsla,
    pub tab_active_background: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub terminal_background: Hsla,
    pub terminal_foreground: Hsla,
    pub cursor: Hsla,
    pub selection: Hsla,
    /// 8 normal colors followed by 8 bright colors
    pub ansi: [Hsla; 16],
    pub ansi_dim: [Hsla; 8],
    pub bright_foreground: Hsla,
    pub dim_foreground: Hsla,
}

impl Default for Theme {
    fn default() -> Self {
        Self::bundled(true)
    }
}

impl Global for Theme {}

/// bundled theme for dark mode
pub const DEFAULT_DARK_THEME: &str = include_str!("../assets/default_theme_dark.jsonc");
/// bundled theme for light mode
pub const DEFAULT_LIGHT_THEME: &str = include_str!("../assets/default_theme_light.jsonc");

impl Theme {
    fn bundled_json(dark: bool) -> &'static str {
        if dark {
            DEFAULT_DARK_THEME
        } else {
            DEFAULT_LIGHT_THEME
        }
    }

    /// bundled dark or light theme
    pub fn bundled(dark: bool) -> Self {
        serde_json_lenient::from_str(Self::bundled_json(dark)).expect("bundled theme is invalid")
    }

    /// parse theme, missing colors come from the bundled theme of the same mode
    pub fn parse(json: &str, dark: bool) -> serde_json_lenient::Result<Self> {
        let mut theme: Value = serde_json_lenient::from_str(Self::bundled_json(dark))?;
        merge(&mut theme, serde_json_lenient::from_str(json)?);
        serde_json_lenient::from_value(theme)
    }

    // relative paths start from the folder with settings.jsonc
    fn resolve(path: &Path) -> PathBuf {
        Settings::path()
            .and_then(|settings| settings.parent().map(|dir| dir.join(path)))
            .unwrap_or_else(|| path.to_path_buf())
    }

    /// load dark or light theme set in settings, using the bundled one when not set or invalid
    pub fn load(settings: &ThemeSettings, dark: bool) -> Self {
        // create both theme files up front, so the unused mode is ready to edit too
        for (path, is_dark) in [(&settings.dark, true), (&settings.light, false)] {
            if let Some(path) = path {
                create_default_file(&Self::resolve(path), Self::bundled_json(is_dark));
            }
        }

        let custom = if dark {
            &settings.dark
        } else {
            &settings.light
        };
        let Some(path) = custom else {
            return Self::bundled(dark);
        };
        let path = Self::resolve(path);
        match std::fs::read_to_string(&path) {
            Ok(json) => Self::parse(&json, dark).unwrap_or_else(|error| {
                eprintln!("invalid theme in {}: {error}", path.display());
                Self::bundled(dark)
            }),
            Err(error) => {
                eprintln!("failed to read theme {}: {error}", path.display());
                Self::bundled(dark)
            }
        }
    }

    /// set active theme from settings mode and system appearance
    pub fn apply(appearance: WindowAppearance, cx: &mut App) {
        let settings = &Settings::get(cx).theme;
        let dark = match settings.mode {
            ThemeMode::System => {
                matches!(
                    appearance,
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                )
            }
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
        };
        cx.set_global(Self::load(settings, dark));
    }

    /// active theme
    pub fn get(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// convert alacritty color to gpui color
    pub fn convert_color(&self, color: &Color) -> Hsla {
        match color {
            Color::Named(NamedColor::DimForeground) => self.dim_foreground,
            // named colors share their index with the 0-15 and 256-267 slots
            Color::Named(named) => self.get_color_at_index(*named as usize),
            Color::Spec(rgb) => rgba_color(rgb.r, rgb.g, rgb.b),
            Color::Indexed(i) => self.get_color_at_index(*i as usize),
        }
    }

    /// convert 8 bit ansi color index to gpui color
    pub fn get_color_at_index(&self, index: usize) -> Hsla {
        match index {
            0..=15 => self.ansi[index],
            // 6x6x6 rgb cube, using xterm steps
            16..=231 => {
                let i = index as u8 - 16;
                let step = |v: u8| if v == 0 { 0 } else { v * 40 + 55 };
                rgba_color(step(i / 36), step((i % 36) / 6), step(i % 6))
            }
            // 24 step grayscale ramp
            232..=255 => {
                let value = (index as u8 - 232) * 10 + 8;
                rgba_color(value, value, value)
            }
            256 => self.terminal_foreground,
            257 => self.terminal_background,
            258 => self.cursor,
            259..=266 => self.ansi_dim[index - 259],
            267 => self.bright_foreground,
            268 => self.ansi[0],
            _ => gpui::black(),
        }
    }
}

fn rgba_color(r: u8, g: u8, b: u8) -> Hsla {
    rgb(u32::from_be_bytes([0, r, g, b])).into()
}

#[cfg(test)]
mod tests {
    use gpui::Rgba;

    use super::*;

    #[test]
    fn indexed_colors_match_xterm() {
        let theme = Theme::default();
        assert_eq!(theme.get_color_at_index(1), rgb(0xe06c75).into());
        // 6x6x6 cube corners and a middle step
        assert_eq!(theme.get_color_at_index(16), rgb(0x000000).into());
        assert_eq!(theme.get_color_at_index(231), rgb(0xffffff).into());
        assert_eq!(
            theme.get_color_at_index(16 + 36 + 6 * 2 + 3),
            rgb(0x5f87af).into()
        );
        // grayscale ramp
        assert_eq!(theme.get_color_at_index(232), rgb(0x080808).into());
        assert_eq!(theme.get_color_at_index(255), rgb(0xeeeeee).into());
    }

    #[test]
    fn named_colors_use_theme() {
        let theme = Theme::default();
        assert_eq!(
            theme.convert_color(&Color::Named(NamedColor::Foreground)),
            theme.terminal_foreground
        );
        assert_eq!(
            theme.convert_color(&Color::Named(NamedColor::BrightRed)),
            rgb(0xea858b).into()
        );
        assert_eq!(
            theme.convert_color(&Color::Named(NamedColor::DimForeground)),
            theme.dim_foreground
        );
    }

    #[test]
    fn bundled_themes_are_commented_and_parse() {
        assert!(DEFAULT_DARK_THEME.contains("//"));
        assert!(DEFAULT_LIGHT_THEME.contains("//"));
        assert_eq!(Theme::default(), Theme::bundled(true));
        assert_ne!(Theme::bundled(true), Theme::bundled(false));
        for dark in [true, false] {
            assert_eq!(Theme::parse("{}", dark).unwrap(), Theme::bundled(dark));
        }
    }

    #[test]
    fn partial_theme_fills_from_bundled_of_same_mode() {
        for dark in [true, false] {
            let theme = Theme::parse("{\n // comment\n \"cursor\": \"#abcdef\",\n}", dark).unwrap();
            let mut expected = Theme::bundled(dark);
            expected.cursor = rgb(0xabcdef).into();
            assert_eq!(theme, expected);
        }
    }

    #[test]
    fn ansi_is_replaced_whole() {
        for dark in [true, false] {
            assert!(Theme::parse(r##"{"ansi": ["#ffffff"]}"##, dark).is_err());
            let ansi = vec!["\"#123456\""; 16].join(",");
            let theme = Theme::parse(&format!(r#"{{"ansi": [{ansi}]}}"#), dark).unwrap();
            assert!(theme.ansi.iter().all(|c| *c == rgb(0x123456).into()));
            assert_eq!(theme.ansi_dim, Theme::bundled(dark).ansi_dim);
        }
    }

    #[test]
    fn rejects_invalid_themes() {
        for json in [
            "{",
            "not json",
            "[]",
            "null",
            r#"{"cursor": "nope"}"#,
            r#"{"cursor": null}"#,
        ] {
            assert!(
                Theme::parse(json, true).is_err(),
                "expected error for {json:?}"
            );
        }
    }

    #[test]
    fn load_reads_custom_theme_files() {
        use crate::settings::tests::{temp_dir, with_config_home};

        let settings = |dark: Option<&Path>, light: Option<&Path>| ThemeSettings {
            mode: ThemeMode::System,
            dark: dark.map(PathBuf::from),
            light: light.map(PathBuf::from),
        };
        let dir = temp_dir("theme_load");
        let config = dir.join("kuterm");
        std::fs::create_dir_all(config.join("themes")).unwrap();
        with_config_home(&dir, || {
            let none = settings(None, None);
            assert_eq!(Theme::load(&none, true), Theme::bundled(true));
            assert_eq!(Theme::load(&none, false), Theme::bundled(false));

            // relative paths start from the config folder
            std::fs::write(
                config.join("themes/dark.jsonc"),
                r##"{"border": "#010203"}"##,
            )
            .unwrap();
            let custom = settings(Some(Path::new("themes/dark.jsonc")), None);
            let mut expected = Theme::bundled(true);
            expected.border = rgb(0x010203).into();
            assert_eq!(Theme::load(&custom, true), expected);
            assert_eq!(Theme::load(&custom, false), Theme::bundled(false));

            // absolute paths are used as is
            let abs = dir.join("abs.jsonc");
            std::fs::write(&abs, r##"{"text": "#0a0b0c"}"##).unwrap();
            let custom = settings(None, Some(&abs));
            assert_eq!(Theme::load(&custom, false).text, rgb(0x0a0b0c).into());

            // missing files are created from the bundled theme of their mode
            let custom = settings(
                Some(Path::new("new_dark.jsonc")),
                Some(Path::new("new_light.jsonc")),
            );
            assert_eq!(Theme::load(&custom, true), Theme::bundled(true));
            let dark = std::fs::read_to_string(config.join("new_dark.jsonc")).unwrap();
            let light = std::fs::read_to_string(config.join("new_light.jsonc")).unwrap();
            assert_eq!(dark, DEFAULT_DARK_THEME);
            assert_eq!(light, DEFAULT_LIGHT_THEME);

            // invalid files and directories fall back to bundled, files untouched
            std::fs::write(config.join("broken.jsonc"), "{ broken").unwrap();
            let custom = settings(Some(Path::new("broken.jsonc")), Some(Path::new("themes")));
            assert_eq!(Theme::load(&custom, true), Theme::bundled(true));
            assert_eq!(Theme::load(&custom, false), Theme::bundled(false));
            assert_eq!(
                std::fs::read_to_string(config.join("broken.jsonc")).unwrap(),
                "{ broken"
            );
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundled_theme_colors_round_trip_exactly() {
        // color replies (osc 4/10/11/12) round the theme hsla back to 8 bit rgb, which must
        // give the exact hex from the theme file
        for dark in [true, false] {
            let value: Value = serde_json_lenient::from_str(Theme::bundled_json(dark)).unwrap();
            let theme = Theme::bundled(dark);
            let c = |v: f32| (v * 255.).round() as u8;
            let hex = |h: Hsla| {
                let rgba: Rgba = h.into();
                format!("#{:02x}{:02x}{:02x}", c(rgba.r), c(rgba.g), c(rgba.b))
            };
            for (i, color) in theme.ansi.iter().enumerate() {
                assert_eq!(
                    hex(*color),
                    value["ansi"][i].as_str().unwrap().to_lowercase(),
                    "dark={dark} ansi {i}"
                );
            }
            assert_eq!(
                hex(theme.terminal_background),
                value["terminal_background"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
            );
            assert_eq!(
                hex(theme.terminal_foreground),
                value["terminal_foreground"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
            );
            assert_eq!(
                hex(theme.cursor),
                value["cursor"].as_str().unwrap().to_lowercase()
            );
        }
    }
}
