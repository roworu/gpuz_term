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

    use gpui::{Bounds, Pixels, point, px, size};

    use super::ScrollbarLayout;

    const TRACK_H: f32 = 200.;

    fn track() -> Bounds<Pixels> {
        Bounds::new(point(px(300.), px(40.)), size(px(10.), px(TRACK_H)))
    }

    fn bar(history: usize, lines: usize, offset: usize) -> ScrollbarLayout {
        ScrollbarLayout::new(track(), history, lines, offset, gpui::black())
    }

    fn inside(b: &ScrollbarLayout) -> bool {
        let t = b.track;
        b.thumb.origin.y >= t.origin.y - px(0.01)
            && b.thumb.origin.y + b.thumb.size.height <= t.origin.y + t.size.height + px(0.01)
    }

    #[test]
    fn thumb_uses_track_x_and_width() {
        for (h, o) in [(0, 0), (50, 10), (5000, 4000)] {
            let b = bar(h, 20, o);
            assert_eq!(b.thumb.origin.x, b.track.origin.x);
            assert_eq!(b.thumb.size.width, b.track.size.width);
            assert_eq!(b.track, track());
        }
    }

    #[test]
    fn thumb_stays_inside_track() {
        for h in [0, 1, 5, 20, 100, 10_000, 1_000_000] {
            for o in [0, h / 3, h / 2, h] {
                let b = bar(h, 20, o);
                assert!(
                    inside(&b),
                    "thumb outside for history {h} offset {o}: {b:?}"
                );
            }
        }
    }

    #[test]
    fn no_history_thumb_is_full_track() {
        let b = bar(0, 30, 0);
        assert_eq!(b.thumb.size.height, px(TRACK_H));
    }

    #[test]
    fn thumb_height_is_proportional_to_visible_part() {
        // 60 history + 20 screen: a quarter of the track
        let b = bar(60, 20, 0);
        assert!((f32::from(b.thumb.size.height) - TRACK_H / 4.).abs() < 0.01);
    }

    #[test]
    fn more_history_gives_smaller_or_equal_thumb() {
        let mut last = f32::MAX;
        for h in [0, 10, 40, 100, 400, 4000] {
            let height = f32::from(bar(h, 20, 0).thumb.size.height);
            assert!(height <= last);
            last = height;
        }
    }

    #[test]
    fn huge_history_thumb_still_grabbable() {
        let b = bar(10_000_000, 20, 0);
        assert!(f32::from(b.thumb.size.height) >= 5.);
    }

    #[test]
    fn bottom_offset_puts_thumb_at_bottom_top_at_top() {
        let b = bar(100, 20, 0);
        let end = b.thumb.origin.y + b.thumb.size.height;
        assert!((f32::from(end - (b.track.origin.y + b.track.size.height))).abs() < 0.01);
        let t = bar(100, 20, 100);
        assert!((f32::from(t.thumb.origin.y - t.track.origin.y)).abs() < 0.01);
    }

    #[test]
    fn larger_offset_moves_thumb_up() {
        let mut last = f32::MAX;
        for o in [0, 10, 25, 50, 75, 100] {
            let y = f32::from(bar(100, 20, o).thumb.origin.y);
            assert!(y < last, "offset {o} did not move thumb up");
            last = y;
        }
    }

    #[test]
    fn offset_at_thumb_center_round_trips() {
        for h in [7, 100, 5000] {
            for o in [0, 1, h / 2, h - 1, h] {
                let b = bar(h, 20, o);
                let mid = b.thumb.origin.y + b.thumb.size.height / 2.;
                let got = b.offset_at(mid) as i64;
                // large histories on a 200px track can lose a line or so to rounding
                let tol = (h as i64 / 150).max(0);
                assert!(
                    (got - o as i64).abs() <= tol,
                    "history {h} offset {o} got {got}"
                );
            }
        }
    }

    #[test]
    fn offset_at_is_clamped_to_history() {
        let b = bar(100, 20, 50);
        assert_eq!(b.offset_at(px(-10_000.)), 100);
        assert_eq!(b.offset_at(px(10_000.)), 0);
        assert_eq!(b.offset_at(b.track.origin.y), 100);
        assert_eq!(b.offset_at(b.track.origin.y + b.track.size.height), 0);
    }

    #[test]
    fn offset_at_decreases_going_down() {
        let b = bar(100, 20, 0);
        let mut last = usize::MAX;
        let mut y = f32::from(b.track.origin.y);
        while y <= f32::from(b.track.origin.y) + TRACK_H {
            let o = b.offset_at(px(y));
            assert!(o <= last);
            last = o;
            y += 5.;
        }
    }

    #[test]
    fn offset_at_without_history_is_zero() {
        let b = bar(0, 20, 0);
        for y in [-50., 40., 140., 240., 900.] {
            assert_eq!(b.offset_at(px(y)), 0);
        }
    }

    #[test]
    fn tiny_track_does_not_panic_or_escape() {
        let t = Bounds::new(point(px(0.), px(0.)), size(px(8.), px(5.)));
        let b = ScrollbarLayout::new(t, 1000, 2, 500, gpui::black());
        assert!(b.thumb.size.height <= px(5.));
        assert!(b.thumb.origin.y >= px(0.));
        let _ = b.offset_at(px(3.));
    }

    #[test]
    fn zero_screen_lines_does_not_panic() {
        let b = bar(10, 0, 5);
        assert!(inside(&b));
        let _ = b.offset_at(px(100.));
    }
}
