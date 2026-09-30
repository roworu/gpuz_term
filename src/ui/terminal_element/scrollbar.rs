//! scrollbar showing where the viewport is in scrollback

use gpui::{Bounds, Hsla, Pixels, Window, fill, point, px, size};

// long histories would shrink the thumb until it can't be grabbed
const MIN_THUMB_HEIGHT: f32 = 20.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarLayout {
    pub track: Bounds<Pixels>,
    pub thumb: Bounds<Pixels>,
    history_size: usize,
    color: Hsla,
}

impl ScrollbarLayout {
    pub fn new(
        track: Bounds<Pixels>,
        history_size: usize,
        screen_lines: usize,
        display_offset: usize,
        color: Hsla,
    ) -> Self {
        let track_height = f32::from(track.size.height);
        let total = (history_size + screen_lines).max(1) as f32;
        let height = (track_height * screen_lines as f32 / total)
            .max(MIN_THUMB_HEIGHT)
            .min(track_height);
        let free = track_height - height;
        // offset 0 is the bottom, so the thumb starts at the end of the track
        let top = free * (1. - display_offset as f32 / history_size.max(1) as f32);
        let thumb = Bounds::new(
            point(track.origin.x, track.origin.y + px(top)),
            size(track.size.width, px(height)),
        );
        Self {
            track,
            thumb,
            history_size,
            color,
        }
    }

    pub fn offset_at(&self, y: Pixels) -> usize {
        let height = f32::from(self.thumb.size.height);
        let free = f32::from(self.track.size.height) - height;
        if free <= 0. {
            return 0;
        }
        let top = (f32::from(y - self.track.origin.y) - height / 2.).clamp(0., free);
        ((1. - top / free) * self.history_size as f32).round() as usize
    }

    pub fn paint(&self, window: &mut Window) {
        let radius = self.thumb.size.width / 2.;
        window.paint_quad(fill(self.thumb, self.color).corner_radii(radius));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(history_size: usize, display_offset: usize) -> ScrollbarLayout {
        // 100px high track and 10 screen lines
        let track = Bounds::new(point(px(50.), px(10.)), size(px(8.), px(100.)));
        ScrollbarLayout::new(track, history_size, 10, display_offset, gpui::black())
    }

    #[test]
    fn thumb_fills_track_without_history() {
        let bar = layout(0, 0);
        assert_eq!(bar.thumb, bar.track);
        assert_eq!(bar.offset_at(px(60.)), 0);
    }

    #[test]
    fn thumb_follows_display_offset() {
        // 40 history + 10 screen lines, so the thumb is a fifth of the track
        let bottom = layout(40, 0);
        assert_eq!(bottom.thumb.size.height, px(20.));
        assert_eq!(bottom.thumb.origin.y, px(90.));
        assert_eq!(bottom.thumb.origin.x, px(50.));
        assert_eq!(layout(40, 40).thumb.origin.y, px(10.));
        assert_eq!(layout(40, 20).thumb.origin.y, px(50.));
    }

    #[test]
    fn thumb_keeps_min_height() {
        let bar = layout(100_000, 0);
        assert_eq!(bar.thumb.size.height, px(MIN_THUMB_HEIGHT));
        assert_eq!(bar.thumb.origin.y, px(90.));
    }

    #[test]
    fn offset_at_is_inverse_of_thumb_position() {
        for offset in [0, 10, 20, 33, 40] {
            let bar = layout(40, offset);
            let middle = bar.thumb.origin.y + bar.thumb.size.height / 2.;
            assert_eq!(bar.offset_at(middle), offset);
        }
        // past the ends of the track clamps to the top and bottom of history
        let bar = layout(40, 0);
        assert_eq!(bar.offset_at(px(-500.)), 40);
        assert_eq!(bar.offset_at(px(500.)), 0);
    }
}
