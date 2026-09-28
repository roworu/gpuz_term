//! turns grid cells into background rects and batched text runs

use alacritty_terminal::{
    selection::SelectionRange,
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
    selection: Option<SelectionRange>,
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

        let selected = selection.is_some_and(|range| range.contains(indexed.point));
        let background = if selected {
            Some(theme.selection)
        } else {
            (!is_default_background(&bg)).then(|| theme.convert_color(&bg))
        };
        if let Some(color) = background {
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
            Some(run) if run.can_append(line, column, &style) => run.push(cell.c, cell.zerowidth()),
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
    // gpui's underline shader applies alpha twice, so pass its square root to match the text
    let line_color = Hsla {
        a: color.a.sqrt(),
        ..color
    };
    let underline = cell
        .flags
        .intersects(Flags::ALL_UNDERLINES)
        .then(|| UnderlineStyle {
            color: Some(line_color),
            thickness: px(1.),
            wavy: cell.flags.contains(Flags::UNDERCURL),
        });
    let strikethrough = cell
        .flags
        .contains(Flags::STRIKEOUT)
        .then(|| StrikethroughStyle {
            color: Some(line_color),
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

#[cfg(test)]
mod tests {
    use alacritty_terminal::{
        Term,
        event::VoidListener,
        term::{Config, test::TermSize},
        vte::ansi::Processor,
    };
    use gpui::{Rgba, font, rgb};

    use super::*;

    fn themes() -> [(&'static str, Theme); 2] {
        [
            ("dark", Theme::bundled(true)),
            ("light", Theme::bundled(false)),
        ]
    }

    /// xterm 256 color palette entry 16-255, computed here instead of asking the theme
    fn xterm(index: u8) -> Hsla {
        let (r, g, b) = match index {
            16..=231 => {
                let i = index - 16;
                let step = |v: u8| if v == 0 { 0 } else { v * 40 + 55 };
                (step(i / 36), step((i % 36) / 6), step(i % 6))
            }
            _ => {
                let v = (index - 232) * 10 + 8;
                (v, v, v)
            }
        };
        rgb(u32::from_be_bytes([0, r, g, b])).into()
    }

    #[track_caller]
    fn assert_color(got: Hsla, want: Hsla, what: &str) {
        let (a, b): (Rgba, Rgba) = (got.into(), want.into());
        let same = (a.r - b.r).abs() < 0.003
            && (a.g - b.g).abs() < 0.003
            && (a.b - b.b).abs() < 0.003
            && (a.a - b.a).abs() < 0.003;
        assert!(same, "{what}: got {a:?}, want {b:?}");
    }

    /// rects and runs of a real 40x3 alacritty term after feeding it `bytes`
    fn layout_with(
        bytes: &str,
        selection: Option<SelectionRange>,
        theme: &Theme,
    ) -> (Vec<LayoutRect>, Vec<BatchedTextRun>) {
        let mut term = Term::new(Config::default(), &TermSize::new(40, 3), VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, bytes.as_bytes());
        let cells: Vec<IndexedCell> = term
            .renderable_content()
            .display_iter
            .map(|indexed| IndexedCell {
                point: indexed.point,
                cell: indexed.cell.clone(),
            })
            .collect();
        layout_grid(&cells, 0, selection, &font("Mono"), theme)
    }

    fn layout(bytes: &str, theme: &Theme) -> Vec<BatchedTextRun> {
        layout_with(bytes, None, theme).1
    }

    /// the text run that paints column `col` of line 0
    fn run_at(runs: &[BatchedTextRun], col: i32) -> &BatchedTextRun {
        runs.iter()
            .find(|run| {
                run.line == 0 && run.column <= col && col < run.column + run.cell_count as i32
            })
            .unwrap_or_else(|| panic!("no text run at column {col}"))
    }

    #[test]
    fn dim_normal_named_colors_use_ansi_dim() {
        let normal = [
            NamedColor::Black,
            NamedColor::Red,
            NamedColor::Green,
            NamedColor::Yellow,
            NamedColor::Blue,
            NamedColor::Magenta,
            NamedColor::Cyan,
            NamedColor::White,
        ];
        for (name, theme) in themes() {
            for (i, named) in normal.into_iter().enumerate() {
                let got = dim_color(Color::Named(named), &theme);
                assert_color(got, theme.ansi_dim[i], &format!("{name} dim {named:?}"));
                assert_eq!(got.a, 1.0, "{name} dim {named:?} is opaque");
            }
        }
    }

    #[test]
    fn dim_indexed_16_to_255_fade() {
        for (name, theme) in themes() {
            for i in 16..=255u8 {
                let got = dim_color(Color::Indexed(i), &theme);
                let mut want = xterm(i);
                want.a *= 0.7;
                assert_color(got, want, &format!("{name} dim 38;5;{i}"));
            }
        }
    }

    #[test]
    fn sgr_22_and_0_end_dim() {
        for (name, theme) in themes() {
            let runs = layout(
                "\x1b[2;31mA\x1b[22mB\x1b[2mC\x1b[0mD\x1b[2mE\x1b[1mF",
                &theme,
            );
            assert_color(run_at(&runs, 0).style.color, theme.ansi_dim[1], name);
            assert_color(run_at(&runs, 1).style.color, theme.ansi[1], name);
            assert_color(run_at(&runs, 2).style.color, theme.ansi_dim[1], name);
            assert_color(
                run_at(&runs, 3).style.color,
                theme.terminal_foreground,
                name,
            );
            assert_color(run_at(&runs, 4).style.color, theme.dim_foreground, name);
            // bold on top of dim keeps dim
            assert_color(run_at(&runs, 5).style.color, theme.dim_foreground, name);
        }
    }

    #[test]
    fn dim_and_normal_cells_are_separate_runs_and_equal_dim_cells_batch() {
        for (name, theme) in themes() {
            let runs = layout("\x1b[31mab\x1b[2mcd\x1b[22mef", &theme);
            let line: Vec<_> = runs.iter().filter(|r| r.line == 0).collect();
            assert_eq!(line.len(), 3, "{name}");
            assert_eq!(line[1].text, "cd");
            assert_eq!(line[1].cell_count, 2);
            assert_color(line[1].style.color, theme.ansi_dim[1], name);
        }
    }

    #[test]
    fn selection_paints_its_cells_including_blanks() {
        use alacritty_terminal::index::{Column, Line, Point as AlacPoint};

        for (name, theme) in themes() {
            // "ab" on red background, then blanks, selected from column 1 to 4
            let range = SelectionRange::new(
                AlacPoint::new(Line(0), Column(1)),
                AlacPoint::new(Line(0), Column(4)),
                false,
            );
            let (rects, runs) = layout_with("\x1b[41mab\x1b[0m", Some(range), &theme);
            let line: Vec<_> = rects.iter().filter(|r| r.line == 0).collect();
            assert_eq!(line.len(), 2, "{name}");
            assert_eq!((line[0].column, line[0].num_of_cells), (0, 1));
            assert_color(line[0].color, theme.ansi[1], name);
            assert_eq!((line[1].column, line[1].num_of_cells), (1, 4));
            assert_color(line[1].color, theme.selection, name);
            // text keeps its color, only the background changes
            assert_color(
                run_at(&runs, 1).style.color,
                theme.terminal_foreground,
                name,
            );
            assert!(rects.iter().all(|r| r.line == 0), "{name}");
        }
    }
}
