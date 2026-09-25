//! default one dark colors

use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::{Hsla, rgb};

pub const TAB_BAR_BACKGROUND: u32 = 0x2f343e;
pub const TAB_ACTIVE_BACKGROUND: u32 = 0x282c33;
pub const BORDER: u32 = 0x464b57;
pub const TEXT: u32 = 0xdce0e5;
pub const TEXT_MUTED: u32 = 0xa9afbc;
pub const TERMINAL_BACKGROUND: u32 = 0x282c34;
pub const TERMINAL_FOREGROUND: u32 = 0xabb2bf;
pub const CURSOR: u32 = 0x74ade8;

const ANSI: [u32; 16] = [
    0x282c34, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf, // normal
    0x636d83, 0xea858b, 0xaad581, 0xffd885, 0x85c1ff, 0xd398eb, 0x6ed5de, 0xfafafa, // bright
];

const ANSI_DIM: [u32; 8] = [
    0x3b3f4a, 0xa7545a, 0x6d8f59, 0xb8985b, 0x457cad, 0x8d54a0, 0x3c818a, 0x8f969b,
];

const BRIGHT_FOREGROUND: u32 = 0xdce0e5;
const DIM_FOREGROUND: u32 = 0x636d83;

/// convert alacritty color to gpui color
pub fn convert_color(color: &Color) -> Hsla {
    match color {
        // zed maps index 268 to dim background, so dim foreground needs its own arm
        Color::Named(NamedColor::DimForeground) => rgb(DIM_FOREGROUND).into(),
        // named colors share their index with the 0-15 and 256-267 slots
        Color::Named(named) => get_color_at_index(*named as usize),
        Color::Spec(rgb) => rgba_color(rgb.r, rgb.g, rgb.b),
        Color::Indexed(i) => get_color_at_index(*i as usize),
    }
}

/// convert 8 bit ansi color index to gpui color
pub fn get_color_at_index(index: usize) -> Hsla {
    match index {
        0..=15 => rgb(ANSI[index]).into(),
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
        256 => rgb(TERMINAL_FOREGROUND).into(),
        257 => rgb(TERMINAL_BACKGROUND).into(),
        258 => rgb(CURSOR).into(),
        259..=266 => rgb(ANSI_DIM[index - 259]).into(),
        267 => rgb(BRIGHT_FOREGROUND).into(),
        268 => rgb(ANSI[0]).into(),
        _ => gpui::black(),
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
        assert_eq!(get_color_at_index(1), rgb(0xe06c75).into());
        // 6x6x6 cube corners and a middle step
        assert_eq!(get_color_at_index(16), rgb(0x000000).into());
        assert_eq!(get_color_at_index(231), rgb(0xffffff).into());
        assert_eq!(get_color_at_index(16 + 36 + 6 * 2 + 3), rgb(0x5f87af).into());
        // grayscale ramp
        assert_eq!(get_color_at_index(232), rgb(0x080808).into());
        assert_eq!(get_color_at_index(255), rgb(0xeeeeee).into());
    }

    #[test]
    fn named_colors_use_theme() {
        assert_eq!(
            convert_color(&Color::Named(NamedColor::Foreground)),
            rgb(TERMINAL_FOREGROUND).into()
        );
        assert_eq!(
            convert_color(&Color::Named(NamedColor::BrightRed)),
            rgb(0xea858b).into()
        );
        assert_eq!(
            convert_color(&Color::Named(NamedColor::DimForeground)),
            rgb(DIM_FOREGROUND).into()
        );
    }
}
