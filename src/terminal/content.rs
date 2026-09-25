//! snapshot of visible grid, read by painter

use alacritty_terminal::{
    Term,
    index::Point as AlacPoint,
    term::{RenderableCursor, TermMode, cell::Cell},
};

use super::{TerminalBounds, builder::ZedListener};

pub struct IndexedCell {
    pub point: AlacPoint,
    pub cell: Cell,
}

/// snapshot of grid, taken once per frame in `sync`
pub struct Content {
    pub cells: Vec<IndexedCell>,
    pub mode: TermMode,
    pub display_offset: usize,
    pub cursor: RenderableCursor,
    pub cursor_char: char,
    pub terminal_bounds: TerminalBounds,
}

impl Default for Content {
    fn default() -> Self {
        Content {
            cells: Vec::new(),
            mode: TermMode::empty(),
            display_offset: 0,
            cursor: RenderableCursor {
                shape: alacritty_terminal::vte::ansi::CursorShape::Block,
                point: AlacPoint::default(),
            },
            cursor_char: ' ',
            terminal_bounds: TerminalBounds::default(),
        }
    }
}

pub(super) fn make_content(term: &Term<ZedListener>, terminal_bounds: TerminalBounds) -> Content {
    let content = term.renderable_content();
    let cells = content
        .display_iter
        .map(|indexed| IndexedCell {
            point: indexed.point,
            cell: indexed.cell.clone(),
        })
        .collect();
    Content {
        cells,
        mode: content.mode,
        display_offset: content.display_offset,
        cursor: content.cursor,
        cursor_char: term.grid()[content.cursor.point].c,
        terminal_bounds,
    }
}
