//! painting cursor in each shape

use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{
    App, Bounds, Pixels, Point, ShapedLine, TextAlign, Window, fill, outline, point, px, size,
};

use crate::theme::Theme;

pub(super) struct CursorLayout {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) shape: CursorShape,
    pub(super) focused: bool,
    pub(super) text: ShapedLine,
}

impl CursorLayout {
    pub(super) fn paint(&self, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        let bounds = self.bounds + origin;
        let color = Theme::get(cx).cursor;
        match self.shape {
            CursorShape::Block if self.focused => {
                window.paint_quad(fill(bounds, color));
                self.text
                    .paint(
                        bounds.origin,
                        bounds.size.height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
            }
            CursorShape::Beam if self.focused => {
                window.paint_quad(fill(
                    Bounds::new(bounds.origin, size(px(2.), bounds.size.height)),
                    color,
                ));
            }
            CursorShape::Underline if self.focused => {
                let origin = point(bounds.origin.x, bounds.bottom() - px(2.));
                window.paint_quad(fill(
                    Bounds::new(origin, size(bounds.size.width, px(2.))),
                    color,
                ));
            }
            // unfocused cursors are drawn hollow
            // TODO: do we need a settings here to change its begaviour?
            _ => window.paint_quad(outline(bounds, color, gpui::BorderStyle::Solid)),
        }
    }
}
