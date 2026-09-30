//! drawing the workspace: tab bar on top, active terminal below
// TODO: need a setting on where to put tabs (top, bottom, left?, right?)

use std::sync::LazyLock;

use gpui::{
    AnyElement, App, Context, Div, MouseButton, ScrollHandle, Stateful, Window, anchored, deferred,
    div, prelude::*, px, rems,
};
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
};

use super::Workspace;
use crate::{
    settings::{NewTabButton, Settings, TabIconPosition, TabTitleAlign},
    theme::Theme,
};

// tab bar text size in rems, the icon offset is scaled by it
const TEXT_SIZE: f32 = 0.875;

/// how far `icon` must move down, in ems, for its lowest point to stand on the baseline
fn icon_drop(icon: &str) -> f32 {
    // nerd font icons are centered on the line, not set on the baseline like letters.
    // gpui can't measure glyph outlines on linux, so they are read from the bundled font
    static FONT: LazyLock<Option<FontRef<'static>>> =
        LazyLock::new(|| FontRef::new(crate::FONT_REGULAR).ok());
    let Some(font) = FONT.as_ref() else {
        return 0.;
    };
    let charmap = font.charmap();
    let glyphs = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let units_per_em = font
        .metrics(Size::unscaled(), LocationRef::default())
        .units_per_em as f32;
    icon.chars()
        .filter_map(|ch| glyphs.bounds(charmap.map(ch)?))
        .map(|bounds| bounds.y_min)
        .reduce(f32::min)
        .map_or(0., |y_min| y_min / units_per_em)
}

/// row holding the tabs, fills the bar next to the new tab button
fn tab_row(scroll: &ScrollHandle) -> Stateful<Div> {
    div()
        .id("tabs")
        .flex()
        .flex_1()
        .min_w_0()
        .h_full()
        // the wheel scrolls hidden tabs into view, it does nothing while all tabs fit.
        // expanded tabs can't shrink below their padding, so many of them overflow too
        .overflow_x_scroll()
        .track_scroll(scroll)
}

/// tab sized from settings, with its icon and title
fn tab(ix: usize, title: String, icon: String, settings: &Settings) -> Stateful<Div> {
    let expand = settings.expand_tabs;
    let icon = div()
        .debug_selector(move || format!("tab-icon-{ix}"))
        .flex_none()
        .relative()
        .top(rems(icon_drop(&icon) * TEXT_SIZE))
        .child(icon);
    let title = div()
        .flex()
        .flex_1()
        .min_w_0()
        .map(|row| match settings.tab_title_align {
            TabTitleAlign::Left => row.justify_start(),
            TabTitleAlign::Center => row.justify_center(),
            TabTitleAlign::Right => row.justify_end(),
        })
        .child(
            div()
                .debug_selector(move || format!("tab-title-{ix}"))
                .min_w_0()
                .truncate()
                .child(title),
        );
    div()
        .id(("tab", ix))
        .flex()
        .items_center()
        .gap_1()
        .h_full()
        .px_3()
        // zero basis with flex_1 splits the row equally, whatever the titles are
        .when(expand, |tab| tab.flex_1().min_w_0())
        // keep their width and overflow the row instead of squeezing together
        .when(!expand, |tab| {
            tab.flex_none().w(px(settings.tab_width as f32))
        })
        .map(|tab| match settings.tab_icon.position {
            TabIconPosition::Left => tab.child(icon).child(title),
            TabIconPosition::Right => tab.child(title).child(icon),
        })
}

impl Workspace {
    /// title shown for the tab at `ix`, also listed by the tab picker
    pub(super) fn tab_title(&self, ix: usize, cx: &App) -> String {
        let tab_state = &self.tabs[ix];
        // the program title stands in until the first refresh, or when the blocks are empty
        if tab_state.title.is_empty() {
            tab_state
                .view
                .read(cx)
                .terminal()
                .read(cx)
                .title(&Settings::get(cx).default_title)
        } else {
            tab_state.title.clone()
        }
    }

    fn render_tab(&self, ix: usize, cx: &Context<Self>) -> Stateful<Div> {
        let is_active = ix == self.active;
        let theme = Theme::get(cx);
        let settings = Settings::get(cx);
        tab(
            ix,
            self.tab_title(ix, cx),
            self.tabs[ix].icon.clone(),
            settings,
        )
        .group("tab")
        .border_r_1()
        .border_color(theme.border)
        .when(is_active, |tab| tab.bg(theme.tab_active_background))
        .text_color(if is_active {
            theme.text
        } else {
            theme.text_muted
        })
        .on_click(cx.listener(move |this, _, window, cx| this.activate_tab(ix, window, cx)))
        .on_mouse_down(
            MouseButton::Middle,
            cx.listener(move |this, _, window, cx| this.request_close_tab(ix, window, cx)),
        )
        .when(settings.show_tab_close_button, |tab| {
            tab.child(
                div()
                    .id(("close-tab", ix))
                    .debug_selector(move || format!("close-tab-{ix}"))
                    .px_1()
                    .rounded_sm()
                    .invisible()
                    .group_hover("tab", |close| close.visible())
                    .when(is_active, |close| close.visible())
                    .hover(|close| close.bg(theme.border))
                    .child("×")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.request_close_tab(ix, window, cx);
                    })),
            )
        })
    }

    /// profile list opened with a right click on "+", picking one opens a tab with it
    fn render_profile_menu(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let position = self.profile_menu?;
        let theme = Theme::get(cx);
        let items = Settings::get(cx)
            .profiles
            .iter()
            .enumerate()
            .map(|(ix, profile)| {
                div()
                    .id(("profile", ix))
                    .debug_selector(move || format!("profile-{ix}"))
                    .px_3()
                    .py_1()
                    .whitespace_nowrap()
                    .hover(|item| item.bg(theme.tab_active_background))
                    .child(profile.name.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.profile_menu = None;
                        let profile = Settings::get(cx).profiles[ix].clone();
                        this.add_profile_tab(&profile, true, window, cx);
                    }))
            });
        let menu = div()
            .id("profile-menu")
            .flex()
            .flex_col()
            .py_1()
            .bg(theme.tab_bar_background)
            .border_1()
            .border_color(theme.border)
            .text_color(theme.text)
            // keeps clicks from reaching the terminal below
            .occlude()
            .children(items)
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.profile_menu = None;
                cx.notify();
            }));
        Some(
            deferred(anchored().position(position).snap_to_window().child(menu)).into_any_element(),
        )
    }

    /// command palette, about page or close tab dialog, centered near the top like in zed
    fn render_overlay(&self) -> Option<AnyElement> {
        let view = match (&self.palette, &self.about, &self.confirm_close) {
            (Some(palette), _, _) => palette.clone().into_any_element(),
            (_, Some(about), _) => about.clone().into_any_element(),
            (_, _, Some(confirm)) => confirm.clone().into_any_element(),
            _ => return None,
        };
        Some(
            deferred(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .justify_center()
                    .items_start()
                    .pt(rems(4.))
                    .child(view),
            )
            .into_any_element(),
        )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = Settings::get(cx);
        let theme = Theme::get(cx);
        // ui scales in rems of the ui font size
        window.set_rem_size(px(settings.ui_font_size));
        let show_bar = !(settings.hide_bar_for_one_tab && self.tabs.len() == 1);
        // fixed width tabs only take the room they need, so the button follows the last one.
        // expanded tabs fill the row anyway, which puts the button at the right end
        let hug_tabs = settings.new_tab_button == NewTabButton::AfterTabs && !settings.expand_tabs;
        let tab_row = tab_row(&self.tab_scroll)
            .when(hug_tabs, |row| row.flex_initial())
            .children((0..self.tabs.len()).map(|ix| self.render_tab(ix, cx)));
        let has_profiles = settings.profiles.len() > 1;
        let new_tab = div()
            .id("new-tab")
            .debug_selector(|| "new-tab".into())
            .flex()
            .items_center()
            .px_3()
            .text_color(theme.text_muted)
            .hover(|button| button.text_color(theme.text))
            .child("+")
            .on_click(cx.listener(|this, _, window, cx| this.add_tab(window, cx)))
            .when(has_profiles, |button| {
                button
                    // hints that a right click picks another profile
                    .child(
                        div()
                            .debug_selector(|| "profile-hint".into())
                            .ml_0p5()
                            .text_xs()
                            .child("▾"),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.profile_menu = Some(event.position);
                            cx.notify();
                        }),
                    )
            });
        div()
            .key_context("Workspace")
            .on_action(cx.listener(Self::new_tab))
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::activate_tab_action))
            .on_action(cx.listener(Self::toggle_command_palette))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.terminal_background)
            .font_family(settings.ui_font_family.clone())
            .text_size(rems(TEXT_SIZE))
            .when(show_bar, |workspace| {
                workspace.child(
                    div()
                        .flex()
                        .flex_none()
                        .h(rems(2.))
                        .bg(theme.tab_bar_background)
                        .border_b_1()
                        .border_color(theme.border)
                        .map(|bar| match settings.new_tab_button {
                            NewTabButton::Left => bar.child(new_tab).child(tab_row),
                            NewTabButton::Right | NewTabButton::AfterTabs => {
                                bar.child(tab_row).child(new_tab)
                            }
                        }),
                )
            })
            .children(
                self.tabs
                    .get(self.active)
                    .map(|tab| div().flex_1().min_h_0().child(tab.view.clone())),
            )
            .children(self.render_profile_menu(cx))
            .children(self.render_notifications(cx))
            .children(self.render_overlay())
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        Bounds, Pixels, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext, point,
        size,
    };

    use super::*;

    /// tab row alone, so layout can be checked without spawning shells
    struct TabRow {
        titles: Vec<String>,
        icon: String,
        settings: Settings,
        scroll: ScrollHandle,
    }

    impl Render for TabRow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let tabs = self
                .titles
                .iter()
                .enumerate()
                .map(|(ix, title)| tab(ix, title.clone(), self.icon.clone(), &self.settings));
            div().size_full().flex().text_size(rems(TEXT_SIZE)).child(
                div()
                    .flex()
                    .h(px(30.))
                    .w(px(600.))
                    .child(tab_row(&self.scroll).children(tabs)),
            )
        }
    }

    /// lay out `count` tabs in a 600px wide row with settings overridden by `json`
    fn layout<'a>(
        json: &str,
        count: usize,
        cx: &'a mut TestAppContext,
    ) -> (gpui::Entity<TabRow>, &'a mut VisualTestContext) {
        let settings = Settings::parse(json).unwrap();
        let (row, cx) = cx.add_window_view(|_, _| TabRow {
            titles: (1..=count).map(|n| format!("tab {n}")).collect(),
            icon: "I".into(),
            settings,
            scroll: ScrollHandle::new(),
        });
        cx.simulate_resize(size(px(1000.), px(600.)));
        cx.run_until_parked();
        (row, cx)
    }

    fn tab_bounds(row: &gpui::Entity<TabRow>, cx: &mut VisualTestContext) -> Vec<Bounds<Pixels>> {
        row.update(cx, |row, _| {
            (0..row.titles.len())
                .map(|ix| row.scroll.bounds_for_item(ix).unwrap())
                .collect()
        })
    }

    fn assert_close(a: Pixels, b: Pixels) {
        // layout snaps to pixels, so equal shares may differ by a fraction
        assert!((a - b).abs() <= px(1.), "{a:?} != {b:?}");
    }

    #[gpui::test]
    fn expanded_tabs_share_the_row_equally(cx: &mut TestAppContext) {
        for count in [1, 2, 3, 7] {
            let (row, cx) = layout(r#"{"expand_tabs": true}"#, count, cx);
            let tabs = tab_bounds(&row, cx);
            for tab in &tabs {
                assert_close(tab.size.width, px(600. / count as f32));
            }
            assert_close(tabs.last().unwrap().right(), tabs[0].left() + px(600.));
        }
    }

    #[gpui::test]
    fn fixed_tabs_use_tab_width(cx: &mut TestAppContext) {
        let (row, cx) = layout(r#"{"expand_tabs": false, "tab_width": 90}"#, 3, cx);
        row.update(cx, |row, cx| {
            row.titles[1] = "a much much much much longer title than the others".into();
            cx.notify();
        });
        cx.run_until_parked();
        for tab in tab_bounds(&row, cx) {
            assert_eq!(tab.size.width, px(90.));
        }
    }

    #[gpui::test]
    fn expanded_tabs_ignore_tab_width(cx: &mut TestAppContext) {
        let (row, cx) = layout(r#"{"expand_tabs": true, "tab_width": 90}"#, 3, cx);
        assert_close(tab_bounds(&row, cx)[0].size.width, px(200.));
    }

    #[gpui::test]
    fn expanded_tabs_ignore_title_length(cx: &mut TestAppContext) {
        let (row, cx) = layout(r#"{"expand_tabs": true}"#, 2, cx);
        row.update(cx, |row, cx| {
            row.titles[0] = "a much much much much longer title than the other one".into();
            cx.notify();
        });
        cx.run_until_parked();
        let tabs = tab_bounds(&row, cx);
        assert_close(tabs[0].size.width, tabs[1].size.width);
    }

    /// wheel down over the row, returns the row's horizontal scroll offset
    fn scroll_down(row: &gpui::Entity<TabRow>, cx: &mut VisualTestContext) -> Pixels {
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(300.), px(15.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(-100.))),
            ..Default::default()
        });
        cx.run_until_parked();
        row.update(cx, |row, _| row.scroll.offset().x)
    }

    #[gpui::test]
    fn wheel_scrolls_overflowing_tabs(cx: &mut TestAppContext) {
        // 10 tabs of 90px overflow the 600px row
        let (row, cx) = layout(r#"{"expand_tabs": false, "tab_width": 90}"#, 10, cx);
        let tabs = tab_bounds(&row, cx);
        assert!(
            tabs.last().unwrap().right() > px(600.),
            "tabs should overflow: {tabs:?}"
        );
        assert_eq!(scroll_down(&row, cx), px(-100.));
        // scrolling stops at the last tab
        for _ in 0..100 {
            scroll_down(&row, cx);
        }
        let max = row.update(cx, |row, _| row.scroll.max_offset().x);
        assert_eq!(scroll_down(&row, cx), -max);
    }

    #[gpui::test]
    fn wheel_does_nothing_for_expanded_tabs(cx: &mut TestAppContext) {
        let (row, cx) = layout(r#"{"expand_tabs": true}"#, 7, cx);
        assert_eq!(scroll_down(&row, cx), px(0.));
        let tabs = tab_bounds(&row, cx);
        assert_close(tabs.last().unwrap().right(), px(600.));
    }

    #[gpui::test]
    fn too_many_expanded_tabs_can_still_be_reached(cx: &mut TestAppContext) {
        let (row, cx) = layout(r#"{"expand_tabs": true}"#, 100, cx);
        assert!(tab_bounds(&row, cx).last().unwrap().right() > px(600.));
        assert_eq!(scroll_down(&row, cx), px(-100.));
    }

    fn title_bounds(cx: &mut VisualTestContext) -> Bounds<Pixels> {
        cx.debug_bounds("tab-title-0").unwrap()
    }

    fn align_json(align: &str) -> String {
        format!(
            r#"{{"expand_tabs": false, "tab_width": 200, "tab_title_align": "{align}",
                "tab_icon": {{"position": "left"}}}}"#
        )
    }

    fn icon_bounds(cx: &mut VisualTestContext) -> Bounds<Pixels> {
        cx.debug_bounds("tab-icon-0").unwrap()
    }

    #[gpui::test]
    fn title_follows_align(cx: &mut TestAppContext) {
        // px_3 padding puts the icon and title 12px inside each tab edge, gap_1 is 4px between them
        let (row, cx) = layout(&align_json("left"), 1, cx);
        let tab = tab_bounds(&row, cx)[0];
        let title = title_bounds(cx);
        let start = icon_bounds(cx).right() + px(4.);
        assert!(title.size.width < px(160.));
        assert_close(title.left(), start);

        let (_, cx) = layout(&align_json("right"), 1, cx);
        assert_close(title_bounds(cx).right(), tab.right() - px(12.));

        let (_, cx) = layout(&align_json("center"), 1, cx);
        let end = tab.right() - px(12.);
        assert_close(title_bounds(cx).center().x, start + (end - start) / 2.);
    }

    #[gpui::test]
    fn long_title_is_cut_for_every_align(cx: &mut TestAppContext) {
        for position in ["left", "right"] {
            for align in ["left", "center", "right"] {
                let json = format!(
                    r#"{{"expand_tabs": false, "tab_width": 200, "tab_title_align": "{align}",
                        "tab_icon": {{"position": "{position}"}}}}"#
                );
                let (row, cx) = layout(&json, 1, cx);
                row.update(cx, |row, cx| {
                    row.titles[0] = "a much much much much much longer title than the tab".into();
                    cx.notify();
                });
                cx.run_until_parked();
                let tab = tab_bounds(&row, cx)[0];
                let title = title_bounds(cx);
                let icon = icon_bounds(cx);
                // the icon keeps its place and width, the title is cut next to it
                assert!(icon.size.width > px(0.));
                if position == "left" {
                    assert_close(icon.left(), tab.left() + px(12.));
                    assert_close(title.left(), icon.right() + px(4.));
                    assert_close(title.right(), tab.right() - px(12.));
                } else {
                    assert_close(title.left(), tab.left() + px(12.));
                    assert_close(title.right(), icon.left() - px(4.));
                    assert_close(icon.right(), tab.right() - px(12.));
                }
            }
        }
    }

    #[gpui::test]
    fn icon_follows_position(cx: &mut TestAppContext) {
        let json = |position: &str| {
            format!(
                r#"{{"expand_tabs": false, "tab_width": 200, "tab_icon": {{"position": "{position}"}}}}"#
            )
        };
        let (row, cx) = layout(&json("left"), 1, cx);
        let tab = tab_bounds(&row, cx)[0];
        let (icon, title) = (icon_bounds(cx), title_bounds(cx));
        assert_close(icon.left(), tab.left() + px(12.));
        assert!(icon.right() <= title.left());

        let (_, cx) = layout(&json("right"), 1, cx);
        let (icon, title) = (icon_bounds(cx), title_bounds(cx));
        assert_close(icon.right(), tab.right() - px(12.));
        assert_close(title.left(), tab.left() + px(12.));
        assert!(title.right() <= icon.left());
    }

    #[test]
    fn icon_drop_puts_glyph_bottom_on_baseline() {
        // letters already stand on the baseline
        assert!(icon_drop("x").abs() < 0.01);
        // nerd icons float above it
        assert!(icon_drop("\u{e795}") > 0.05);
        assert!(icon_drop("\u{f06a9}") > 0.);
        // the lowest glyph decides, so a mixed icon keeps the letter on the baseline
        assert_eq!(icon_drop("x\u{e795}"), icon_drop("x"));
        // letters with descenders move up
        assert!(icon_drop("g") < 0.);
        // nothing to measure
        assert_eq!(icon_drop(""), 0.);
        assert_eq!(icon_drop("\u{10fffd}"), 0.);
    }

    #[gpui::test]
    fn icon_is_moved_down_to_the_baseline(cx: &mut TestAppContext) {
        let (row, cx) = layout(r#"{"expand_tabs": false, "tab_width": 200}"#, 1, cx);
        let letter_top = icon_bounds(cx).top();
        assert_close(letter_top, title_bounds(cx).top());
        row.update(cx, |row, cx| {
            row.icon = "\u{e795}".into();
            cx.notify();
        });
        cx.run_until_parked();
        // rems are 16px in the test window
        let drop = px(icon_drop("\u{e795}") * TEXT_SIZE * 16.);
        assert!(drop > px(0.));
        assert_close(icon_bounds(cx).top(), title_bounds(cx).top() + drop);
    }
}
