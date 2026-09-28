//! drawing the workspace: tab bar on top, active terminal below
// TODO: need a setting on where to put tabs (top, bottom, left?, right?)

use gpui::{AnyElement, Context, Div, ScrollHandle, Stateful, Window, div, prelude::*, px, rems};

use super::Workspace;
use crate::{
    settings::{NewTabButton, Settings, TabTitleAlign},
    theme::Theme,
};

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

/// tab sized from settings, with its title
fn tab(ix: usize, title: String, settings: &Settings) -> Stateful<Div> {
    let expand = settings.expand_tabs;
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
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .map(|row| match settings.tab_title_align {
                    TabTitleAlign::Left => row.justify_start(),
                    TabTitleAlign::Center => row.justify_center(),
                    TabTitleAlign::Right => row.justify_end(),
                })
                // shrinks below its text, so long titles are cut at the end whatever the align
                .child(
                    div()
                        .debug_selector(move || format!("tab-title-{ix}"))
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(title),
                ),
        )
}

impl Workspace {
    fn render_tab(&self, ix: usize, cx: &Context<Self>) -> AnyElement {
        let is_active = ix == self.active;
        let theme = Theme::get(cx);
        let tab_state = &self.tabs[ix];
        // the program title stands in until the first refresh, or when the blocks are empty
        let title = if tab_state.title.is_empty() {
            tab_state
                .view
                .read(cx)
                .terminal()
                .read(cx)
                .title(&Settings::get(cx).default_title)
        } else {
            tab_state.title.clone()
        };
        tab(ix, title, Settings::get(cx))
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
            .child(
                div()
                    .id(("close-tab", ix))
                    .px_1()
                    .rounded_sm()
                    .invisible()
                    .group_hover("tab", |close| close.visible())
                    .when(is_active, |close| close.visible())
                    .hover(|close| close.bg(theme.border))
                    .child("×")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_tab_at(ix, window, cx);
                    })),
            )
            .into_any_element()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = Settings::get(cx);
        let theme = Theme::get(cx);
        let ui_font_family = settings.ui_font_family.clone();
        // ui scales in rems of the ui font size
        window.set_rem_size(px(settings.ui_font_size));
        let show_bar = !(settings.hide_bar_for_one_tab && self.tabs.len() == 1);
        let tabs: Vec<_> = (0..self.tabs.len())
            .map(|ix| self.render_tab(ix, cx))
            .collect();
        // fixed width tabs only take the room they need, so the button follows the last one.
        // expanded tabs fill the row anyway, which puts the button at the right end
        let hug_tabs = settings.new_tab_button == NewTabButton::AfterTabs && !settings.expand_tabs;
        let tab_row = tab_row(&self.tab_scroll)
            .when(hug_tabs, |row| row.flex_initial())
            .children(tabs);
        let new_tab = div()
            .id("new-tab")
            .flex()
            .items_center()
            .px_3()
            .text_color(theme.text_muted)
            .hover(|button| button.text_color(theme.text))
            .child("+")
            .on_click(cx.listener(|this, _, window, cx| this.add_tab(window, cx)));
        div()
            .key_context("Workspace")
            .on_action(cx.listener(Self::new_tab))
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::activate_tab_action))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.terminal_background)
            .font_family(ui_font_family)
            .text_sm()
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
        settings: Settings,
        scroll: ScrollHandle,
    }

    impl Render for TabRow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let tabs = self
                .titles
                .iter()
                .enumerate()
                .map(|(ix, title)| tab(ix, title.clone(), &self.settings));
            div().size_full().flex().child(
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
        format!(r#"{{"expand_tabs": false, "tab_width": 200, "tab_title_align": "{align}"}}"#)
    }

    #[gpui::test]
    fn title_follows_align(cx: &mut TestAppContext) {
        // px_3 padding puts the title 12px inside each tab edge
        let (row, cx) = layout(&align_json("left"), 1, cx);
        let tab = tab_bounds(&row, cx)[0];
        let title = title_bounds(cx);
        assert!(title.size.width < px(176.));
        assert_close(title.left(), tab.left() + px(12.));

        let (_, cx) = layout(&align_json("right"), 1, cx);
        assert_close(title_bounds(cx).right(), tab.right() - px(12.));

        let (_, cx) = layout(&align_json("center"), 1, cx);
        assert_close(title_bounds(cx).center().x, tab.center().x);
    }

    #[gpui::test]
    fn long_title_is_cut_for_every_align(cx: &mut TestAppContext) {
        for align in ["left", "center", "right"] {
            let (row, cx) = layout(&align_json(align), 1, cx);
            row.update(cx, |row, cx| {
                row.titles[0] = "a much much much much much longer title than the tab".into();
                cx.notify();
            });
            cx.run_until_parked();
            let tab = tab_bounds(&row, cx)[0];
            let title = title_bounds(cx);
            assert_close(title.left(), tab.left() + px(12.));
            assert_close(title.right(), tab.right() - px(12.));
        }
    }
}
