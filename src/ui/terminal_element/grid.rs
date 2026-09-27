//! turns grid cells into background rects and batched text runs

use alacritty_terminal::{
    term::cell::{Cell, Flags},
    vte::ansi::{Color, NamedColor},
};
use gpui::{
    App, Bounds, Font, FontStyle, FontWeight, Hsla, Pixels, Point, StrikethroughStyle, TextAlign,
    TextRun, UnderlineStyle, Window, fill, point, px, size,
};

use crate::{
    terminal::{IndexedCell, TerminalBounds},
    theme::Theme,
};

/// adjacent cells with same style, shaped and painted as one line
pub(super) struct BatchedTextRun {
    line: i32,
    column: i32,
    text: String,
    cell_count: usize,
    style: TextRun,
}

impl BatchedTextRun {
    fn can_append(&self, line: i32, column: i32, style: &TextRun) -> bool {
        self.line == line
            && self.column + self.cell_count as i32 == column
            && self.style.font == style.font
            && self.style.color == style.color
            && self.style.underline == style.underline
            && self.style.strikethrough == style.strikethrough
    }

    fn push(&mut self, c: char, zerowidth: Option<&[char]>) {
        self.text.push(c);
        self.style.len += c.len_utf8();
        self.cell_count += 1;
        for &c in zerowidth.unwrap_or_default() {
            self.text.push(c);
            self.style.len += c.len_utf8();
        }
    }

    pub(super) fn paint(
        &self,
        origin: Point<Pixels>,
        dims: &TerminalBounds,
        font_size: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let pos = point(
            origin.x + self.column as f32 * dims.cell_width,
            origin.y + self.line as f32 * dims.line_height,
        );
        // force_width keeps every glyph on the cell grid
        window
            .text_system()
            .shape_line(
                self.text.clone().into(),
                font_size,
                std::slice::from_ref(&self.style),
                Some(dims.cell_width),
            )
            .paint(pos, dims.line_height, TextAlign::Left, None, window, cx)
            .ok();
    }
}

/// single line background fill spanning some cells
pub(super) struct LayoutRect {
    line: i32,
    column: i32,
    num_of_cells: usize,
    color: Hsla,
}

impl LayoutRect {
    pub(super) fn paint(&self, origin: Point<Pixels>, dims: &TerminalBounds, window: &mut Window) {
        let position = point(
            (origin.x + self.column as f32 * dims.cell_width).floor(),
            origin.y + self.line as f32 * dims.line_height,
        );
        let size = size(
            (dims.cell_width * self.num_of_cells as f32).ceil(),
            dims.line_height,
        );
        window.paint_quad(fill(Bounds::new(position, size), self.color));
    }
}

pub(super) fn layout_grid(
    cells: &[IndexedCell],
    display_offset: usize,
    font: &Font,
    theme: &Theme,
) -> (Vec<LayoutRect>, Vec<BatchedTextRun>) {
    let mut rects: Vec<LayoutRect> = Vec::new();
    let mut runs: Vec<BatchedTextRun> = Vec::new();

    for indexed in cells {
        let cell = &indexed.cell;
        let line = indexed.point.line.0 + display_offset as i32;
        let column = indexed.point.column.0 as i32;

        let (mut fg, mut bg) = (cell.fg, cell.bg);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }

        if !is_default_background(&bg) {
            let color = theme.convert_color(&bg);
            match rects.last_mut() {
                Some(last)
                    if last.color == color
                        && last.line == line
                        && last.column + last.num_of_cells as i32 == column =>
                {
                    last.num_of_cells += 1
                }
                _ => rects.push(LayoutRect {
                    line,
                    column,
                    num_of_cells: 1,
                    color,
                }),
            }
        }

        // wide chars take two cells, the spacer is only a placeholder
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) || is_blank(cell) {
            continue;
        }

        let style = cell_style(cell, fg, font, theme);
        match runs.last_mut() {
            Some(run) if run.can_append(line, column, &style) => {
                run.push(cell.c, cell.zerowidth())
            }
            _ => {
                let mut run = BatchedTextRun {
                    line,
                    column,
                    text: String::new(),
                    cell_count: 0,
                    style: TextRun { len: 0, ..style },
                };
                run.push(cell.c, cell.zerowidth());
                runs.push(run);
            }
        }
    }
    (rects, runs)
}

fn is_default_background(color: &Color) -> bool {
    matches!(color, Color::Named(NamedColor::Background))
}

fn is_blank(cell: &Cell) -> bool {
    cell.c == ' '
        && is_default_background(&cell.bg)
        && !cell
            .flags
            .intersects(Flags::ALL_UNDERLINES | Flags::STRIKEOUT | Flags::INVERSE)
}

fn dim_color(fg: Color, theme: &Theme) -> Hsla {
    match fg {
        Color::Named(named) if named.to_dim() != named => {
            theme.convert_color(&Color::Named(named.to_dim()))
        }
        Color::Indexed(index @ 0..=7) => theme.get_color_at_index(259 + index as usize),
        Color::Indexed(index @ 8..=15) => theme.get_color_at_index(index as usize - 8),
        _ => {
            let mut color = theme.convert_color(&fg);
            color.a *= 0.7;
            color
        }
    }
}

fn cell_style(cell: &Cell, fg: Color, font: &Font, theme: &Theme) -> TextRun {
    let color = if cell.flags.contains(Flags::DIM) {
        dim_color(fg, theme)
    } else {
        theme.convert_color(&fg)
    };
    let underline = cell.flags.intersects(Flags::ALL_UNDERLINES).then(|| UnderlineStyle {
        color: Some(color),
        thickness: px(1.),
        wavy: cell.flags.contains(Flags::UNDERCURL),
    });
    let strikethrough = cell.flags.contains(Flags::STRIKEOUT).then(|| StrikethroughStyle {
        color: Some(color),
        thickness: px(1.),
    });
    let font = Font {
        weight: if cell.flags.contains(Flags::BOLD) {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        },
        style: if cell.flags.contains(Flags::ITALIC) {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        },
        ..font.clone()
    };
    TextRun {
        len: cell.c.len_utf8(),
        font,
        color,
        background_color: None,
        underline,
        strikethrough,
    }
}
