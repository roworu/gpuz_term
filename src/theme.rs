//! theme colors, dark or light from bundled or custom jsonc files

use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::{App, Global, Hsla, WindowAppearance, rgb};
use serde::Deserialize;
use serde_json_lenient::Value;

use crate::settings::{Settings, ThemeMode, ThemeSettings, merge, create_default_file};

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
        if dark { DEFAULT_DARK_THEME } else { DEFAULT_LIGHT_THEME }
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

    /// load dark or light theme set in settings, using the bundled one when not set or invalid
    pub fn load(settings: &ThemeSettings, dark: bool) -> Self {
        let custom = if dark { &settings.dark } else { &settings.light };
        let Some(path) = custom else {
            return Self::bundled(dark);
        };
        // relative paths start from the folder with settings.jsonc
        let path = Settings::path()
            .and_then(|settings| settings.parent().map(|dir| dir.join(path)))
            .unwrap_or_else(|| path.clone());
        // a missing theme file starts as a copy of the bundled one, ready to edit
        create_default_file(&path, Self::bundled_json(dark));
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
                matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark)
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
            // zed maps index 268 to dim background, so dim foreground needs its own arm
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
    use super::*;

    #[test]
    fn indexed_colors_match_xterm() {
        let theme = Theme::default();
        assert_eq!(theme.get_color_at_index(1), rgb(0xe06c75).into());
        // 6x6x6 cube corners and a middle step
        assert_eq!(theme.get_color_at_index(16), rgb(0x000000).into());
        assert_eq!(theme.get_color_at_index(231), rgb(0xffffff).into());
        assert_eq!(theme.get_color_at_index(16 + 36 + 6 * 2 + 3), rgb(0x5f87af).into());
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
}
