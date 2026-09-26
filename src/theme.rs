//! theme colors, read from a json file, default is one dark

use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::{App, Global, Hsla, rgb};
use serde::Deserialize;

use crate::settings::Settings;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
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
        Self {
            tab_bar_background: rgb(0x2f343e).into(),
            tab_active_background: rgb(0x282c33).into(),
            border: rgb(0x464b57).into(),
            text: rgb(0xdce0e5).into(),
            text_muted: rgb(0xa9afbc).into(),
            terminal_background: rgb(0x282c34).into(),
            terminal_foreground: rgb(0xabb2bf).into(),
            cursor: rgb(0x74ade8).into(),
            ansi: [
                0x282c34, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf, // normal
                0x636d83, 0xea858b, 0xaad581, 0xffd885, 0x85c1ff, 0xd398eb, 0x6ed5de, 0xfafafa, // bright
            ]
            .map(|color| rgb(color).into()),
            ansi_dim: [
                0x3b3f4a, 0xa7545a, 0x6d8f59, 0xb8985b, 0x457cad, 0x8d54a0, 0x3c818a, 0x8f969b,
            ]
            .map(|color| rgb(color).into()),
            bright_foreground: rgb(0xdce0e5).into(),
            dim_foreground: rgb(0x636d83).into(),
        }
    }
}

impl Global for Theme {}

impl Theme {
    /// theme.json next to settings.json
    pub fn path() -> Option<std::path::PathBuf> {
        Some(Settings::path()?.with_file_name("theme.json"))
    }

    /// parse theme, we allow comments and trailing commas
    pub fn parse(json: &str) -> serde_json_lenient::Result<Self> {
        serde_json_lenient::from_str(json)
    }

    /// load theme from the theme file, using defaults when it is missing or invalid
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(json) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        Self::parse(&json).unwrap_or_else(|error| {
            eprintln!("invalid theme in {}: {error}", path.display());
            Self::default()
        })
    }

    /// theme loaded at startup
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
