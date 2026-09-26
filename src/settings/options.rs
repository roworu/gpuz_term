//! value types for settings that have a fixed set of choices

use alacritty_terminal::vte::ansi::CursorShape as AlacCursorShape;
use serde::Deserialize;

/// shell to launch
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Shell {
    /// the user's login shell from /etc/passwd
    System,
    Program(String),
    WithArguments { program: String, args: Vec<String> },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LineHeight {
    Comfortable,
    Standard,
    Custom(f32),
}

impl LineHeight {
    /// line height as a multiple of the font size
    pub fn value(&self) -> f32 {
        match self {
            LineHeight::Comfortable => 1.618,
            LineHeight::Standard => 1.3,
            LineHeight::Custom(value) => *value,
        }
    }
}

/// which theme to use
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// follow system dark/light preference
    System,
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CursorShape {
    Block,
    Bar,
    Underline,
    Hollow,
}

impl From<CursorShape> for AlacCursorShape {
    fn from(shape: CursorShape) -> Self {
        match shape {
            CursorShape::Block => AlacCursorShape::Block,
            CursorShape::Bar => AlacCursorShape::Beam,
            CursorShape::Underline => AlacCursorShape::Underline,
            CursorShape::Hollow => AlacCursorShape::HollowBlock,
        }
    }
}
