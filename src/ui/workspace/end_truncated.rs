//! one line of text cut at its end with an ellipsis when its final width is too small

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, ShapedLine, SharedString, Style, TextAlign, Window, px,
};

const ELLIPSIS: &str = "…";

/// longest start of `text` that fits with an ellipsis after it, or all of it when it fits.
/// `fits` must be monotonic: a shorter text never needs more room than a longer one
fn truncate_end(text: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(text) {
        return text.to_string();
    }
    let cut = |end: usize| format!("{}{ELLIPSIS}", text[..end].trim_end());
    // char boundaries, so multi byte chars are never split
    let ends: Vec<usize> = text.char_indices().map(|(ix, _)| ix).collect();
    // binary search for the last end whose cut still fits, the empty cut is the fallback
    let (mut lo, mut hi) = (0, ends.len());
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if fits(&cut(ends[mid])) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    cut(ends[lo])
}

/// text that lays out at its full width, so flex rows size and align it like plain text,
/// and is cut to its final bounds when painted.
/// gpui's own text truncation cuts to whatever width the last layout probe asked about,
/// which is often narrower than the final one
pub(super) struct EndTruncated {
    text: SharedString,
}

impl EndTruncated {
    pub(super) fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }
}

impl IntoElement for EndTruncated {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn shape(text: &str, window: &Window) -> ShapedLine {
    let style = window.text_style();
    let font_size = style.font_size.to_pixels(window.rem_size());
    let text = SharedString::from(text.to_string());
    let run = style.to_run(text.len());
    window
        .text_system()
        .shape_line(text, font_size, &[run], None)
}

impl Element for EndTruncated {
    type RequestLayoutState = ();
    type PrepaintState = ShapedLine;

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
    ) -> (LayoutId, ()) {
        let full = shape(&self.text, window);
        let height = window.text_style().line_height_in_pixels(window.rem_size());
        let mut style = Style::default();
        style.size.width = full.width.into();
        style.size.height = height.into();
        // shrinks below its text, the cut happens in prepaint
        style.min_size.width = px(0.).into();
        style.flex_shrink = 1.;
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> ShapedLine {
        // a fraction of a pixel is layout rounding, not a text that does not fit
        let width = bounds.size.width + px(0.5);
        let text = truncate_end(&self.text, |text| shape(text, window).width <= width);
        shape(&text, window)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        line: &mut ShapedLine,
        window: &mut Window,
        cx: &mut App,
    ) {
        line.paint(
            bounds.origin,
            bounds.size.height,
            TextAlign::Left,
            None,
            window,
            cx,
        )
        .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // one unit per char, like a monospace font
    fn fits(width: usize) -> impl Fn(&str) -> bool {
        move |text: &str| text.chars().count() <= width
    }

    #[test]
    fn text_that_fits_is_kept_whole() {
        assert_eq!(truncate_end("short", fits(5)), "short");
        assert_eq!(truncate_end("short", fits(50)), "short");
        assert_eq!(truncate_end("", fits(0)), "");
    }

    #[test]
    fn long_text_is_cut_only_at_the_end() {
        let text = "a long title with words";
        for width in 1..text.len() {
            let cut = truncate_end(text, fits(width));
            let kept = cut.strip_suffix(ELLIPSIS).expect("no ellipsis");
            assert!(text.starts_with(kept), "{cut:?} is not a start of {text:?}");
            assert!(fits(width)(&cut), "{cut:?} does not fit {width}");
            // as much as possible is kept: the longest fitting cut, found by trying all
            let best = (0..=text.len())
                .map(|end| format!("{}{ELLIPSIS}", text[..end].trim_end()))
                .filter(|cut| fits(width)(cut))
                .max_by_key(|cut| cut.len())
                .unwrap();
            assert_eq!(cut, best, "width {width}");
        }
    }

    #[test]
    fn spaces_before_the_ellipsis_are_dropped() {
        assert_eq!(truncate_end("some folder name", fits(6)), "some…");
    }

    #[test]
    fn wide_chars_are_never_split() {
        let text = "日本語テキスト";
        let cut = truncate_end(text, fits(4));
        assert_eq!(cut, "日本語…");
    }

    #[test]
    fn nothing_fits_leaves_the_ellipsis() {
        assert_eq!(truncate_end("title", fits(1)), ELLIPSIS);
        assert_eq!(truncate_end("title", fits(0)), ELLIPSIS);
    }
}
