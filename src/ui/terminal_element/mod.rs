//! custom gpui element that paints terminal grid

mod cursor;
mod grid;
mod input_handler;

use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{
    App, Bounds, ContentMask, CursorStyle, DispatchPhase, Element, ElementId, Entity, FocusHandle,
    Font, FontFeatures, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Style, TextRun,
    Window, fill, point, px, relative, size,
};

use crate::{
    settings::Settings,
    terminal::{Terminal, TerminalBounds},
    ui::terminal_view::TerminalView,
};
use cursor::CursorLayout;
use grid::{BatchedTextRun, LayoutRect, layout_grid};
use input_handler::TerminalInputHandler;

/// everything computed in prepaint that paint needs
pub struct LayoutState {
    hitbox: Hitbox,
    rects: Vec<LayoutRect>,
    batched_text_runs: Vec<BatchedTextRun>,
    cursor: Option<CursorLayout>,
    dimensions: TerminalBounds,
    font_size: Pixels,
}

pub struct TerminalElement {
    terminal: Entity<Terminal>,
    terminal_view: Entity<TerminalView>,
    focus: FocusHandle,
    focused: bool,
}

impl TerminalElement {
    /// create element that paints given terminal
    pub fn new(
        terminal: Entity<Terminal>,
        terminal_view: Entity<TerminalView>,
        focus: FocusHandle,
        focused: bool,
    ) -> Self {
        Self {
            terminal,
            terminal_view,
            focus,
            focused,
        }
    }

    // window level listeners, so a drag keeps selecting after leaving the terminal area
    fn register_mouse_listeners(&self, hitbox: Hitbox, window: &mut Window) {
        let view = self.terminal_view.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && hitbox.is_hovered(window)
            {
                view.update(cx, |view, cx| view.mouse_down(event, cx));
            }
        });
        let view = self.terminal_view.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
            if phase == DispatchPhase::Bubble && event.pressed_button == Some(MouseButton::Left) {
                view.update(cx, |view, cx| view.mouse_drag(event, cx));
            }
        });
        let view = self.terminal_view.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                view.update(cx, |view, _| view.mouse_up());
            }
        });
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = LayoutState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let settings = &Settings::get(cx).terminal;
        let font = Font {
            features: FontFeatures::disable_ligatures(),
            ..gpui::font(settings.font_family.clone())
        };
        let font_size = px(settings.font_size);
        let line_height = (font_size * settings.line_height.value()).round();
        let text_system = cx.text_system();
        let font_id = text_system.resolve_font(&font);
        let cell_width = text_system.advance(font_id, font_size, 'm').unwrap().width;

        let mut origin = bounds.origin;
        origin.x += cell_width;
        let mut grid_size = bounds.size;
        grid_size.width = (grid_size.width - cell_width).max(cell_width * 2.);
        // alacritty panics on a grid without rows
        grid_size.height = grid_size.height.max(line_height);

        // snap to device pixels so glyphs do not jitter while resizing
        let scale_factor = window.scale_factor();
        let snap = |v: Pixels| px((f32::from(v) * scale_factor).floor() / scale_factor);
        origin = point(snap(origin.x), snap(origin.y));

        let dimensions =
            TerminalBounds::new(line_height, cell_width, Bounds::new(origin, grid_size));

        self.terminal.update(cx, |terminal, _| {
            terminal.set_size(dimensions);
            terminal.sync();
        });

        let terminal = self.terminal.read(cx);
        let theme = terminal.theme(cx);
        let content = &terminal.last_content;
        let (rects, batched_text_runs) = layout_grid(
            &content.cells,
            content.display_offset,
            content.selection,
            &font,
            theme,
        );

        let cursor_line = content.cursor.point.line.0 + content.display_offset as i32;
        let cursor = (content.cursor.shape != CursorShape::Hidden
            && cursor_line >= 0
            && (cursor_line as usize) < dimensions.num_lines())
        .then(|| {
            let text = window.text_system().shape_line(
                content.cursor_char.to_string().into(),
                font_size,
                &[TextRun {
                    len: content.cursor_char.len_utf8(),
                    font: font.clone(),
                    color: theme.terminal_background,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            // wide glyphs like emoji need a wider block
            let width = if content.cursor_char.is_whitespace() {
                cell_width
            } else {
                text.width.max(cell_width)
            };
            CursorLayout {
                bounds: Bounds::new(
                    point(
                        (content.cursor.point.column.0 as f32 * cell_width).floor(),
                        (cursor_line as f32 * line_height).floor(),
                    ),
                    size(width.ceil(), line_height),
                ),
                shape: content.cursor.shape,
                color: theme.cursor,
                focused: self.focused,
                text,
            }
        });

        LayoutState {
            hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
            rects,
            batched_text_runs,
            cursor,
            dimensions,
            font_size,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.set_cursor_style(CursorStyle::IBeam, &layout.hitbox);
        self.register_mouse_listeners(layout.hitbox.clone(), window);

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let background = self.terminal.read(cx).theme(cx).terminal_background;
            window.paint_quad(fill(bounds, background));

            let origin = layout.dimensions.bounds.origin;
            window.handle_input(
                &self.focus,
                TerminalInputHandler {
                    terminal_view: self.terminal_view.clone(),
                    cursor_bounds: layout.cursor.as_ref().map(|c| c.bounds + origin),
                },
                cx,
            );

            for rect in &layout.rects {
                rect.paint(origin, &layout.dimensions, window);
            }
            for run in &mut layout.batched_text_runs {
                run.paint(origin, &layout.dimensions, layout.font_size, window, cx);
            }
            if let Some(cursor) = &layout.cursor {
                cursor.paint(origin, window, cx);
            }
        });
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
