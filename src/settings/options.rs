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
    WithArguments {
        program: String,
        args: Vec<String>,
    },
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

/// one piece of the tab title
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TabTitleBlock {
    /// tab position, starting at 1
    Number,
    /// user@host
    Prompt,
    Folder,
    /// name of the running program
    Command,
    /// title set by the running program
    Title,
    Text(String),
    /// first line of a shell command's output, run in the current folder
    Exec(String),
}

/// where the title sits inside its tab
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TabTitleAlign {
    Left,
    Center,
    Right,
}

/// where the new tab button sits in the tab bar
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NewTabButton {
    Left,
    Right,
    AfterTabs,
}

/// which side of the tab the icon sits on
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TabIconPosition {
    Left,
    Right,
}

/// when the scrollbar is shown
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScrollbarEnable {
    On,
    Off,
    /// only while there is history to scroll
    Dynamic,
}

/// which side of the terminal the scrollbar sits on
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScrollbarPlacement {
    Left,
    Right,
}
