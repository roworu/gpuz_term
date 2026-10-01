//! terminal size in pixels and in cells

use alacritty_terminal::{event::WindowSize, grid::Dimensions};
use gpui::{Bounds, Pixels, px, size};

/// size of terminal grid in pixels and cells
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalBounds {
    pub cell_width: Pixels,
    pub line_height: Pixels,
    pub bounds: Bounds<Pixels>,
}

impl TerminalBounds {
    /// create bounds from cell metrics and pixel area
    pub fn new(line_height: Pixels, cell_width: Pixels, bounds: Bounds<Pixels>) -> Self {
        Self {
            cell_width,
            line_height,
            bounds,
        }
    }

    /// number of visible rows
    pub fn num_lines(&self) -> usize {
        (self.bounds.size.height / self.line_height).floor() as usize
    }

    /// number of visible columns
    pub fn num_columns(&self) -> usize {
        (self.bounds.size.width / self.cell_width).floor() as usize
    }
}

impl Default for TerminalBounds {
    fn default() -> Self {
        TerminalBounds::new(
            px(5.),
            px(5.),
            Bounds::new(gpui::Point::default(), size(px(500.), px(30.))),
        )
    }
}

impl Dimensions for TerminalBounds {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        self.num_lines()
    }

    fn columns(&self) -> usize {
        self.num_columns()
    }
}

impl From<TerminalBounds> for WindowSize {
    fn from(bounds: TerminalBounds) -> Self {
        WindowSize {
            num_lines: bounds.num_lines() as u16,
            num_cols: bounds.num_columns() as u16,
            cell_width: f32::from(bounds.cell_width) as u16,
            cell_height: f32::from(bounds.line_height) as u16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_lines_match_visible_rows() {
        let bounds = TerminalBounds::new(
            px(20.),
            px(10.),
            Bounds::new(gpui::Point::default(), size(px(800.), px(480.))),
        );
        assert_eq!(bounds.screen_lines(), 24);
        assert_eq!(bounds.total_lines(), bounds.screen_lines());
        assert_eq!(bounds.columns(), 80);

        let window_size = WindowSize::from(bounds);
        assert_eq!(window_size.num_lines, 24);
        assert_eq!(window_size.num_cols, 80);
        assert_eq!(window_size.cell_width, 10);
        assert_eq!(window_size.cell_height, 20);
    }
}
