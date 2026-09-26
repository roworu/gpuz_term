//! drawing the workspace: tab bar on top, active terminal below
// TODO: need a setting on where to put tabs (top, bottom, left?, right?)
// TODO: need a setting on dynamycally show/hide tab bar if only one tab exists

use gpui::{AnyElement, Context, Window, div, prelude::*, px, rems};

use super::Workspace;
use crate::{settings::Settings, theme::Theme};

impl Workspace {
    fn render_tab(&self, ix: usize, cx: &Context<Self>) -> AnyElement {
        let is_active = ix == self.active;
        let theme = Theme::get(cx);
        let title = self.tabs[ix].view.read(cx).terminal().read(cx).title();
        div()
            .id(("tab", ix))
            .group("tab")
            .flex()
            .items_center()
            .gap_1()
            .h_full()
            .px_3()
            .border_r_1()
            .border_color(theme.border)
            .when(is_active, |tab| tab.bg(theme.tab_active_background))
            .text_color(if is_active { theme.text } else { theme.text_muted })
            .on_click(cx.listener(move |this, _, window, cx| this.activate_tab(ix, window, cx)))
            .child(div().max_w(rems(12.5)).overflow_hidden().whitespace_nowrap().child(title))
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
        let tabs: Vec<_> = (0..self.tabs.len())
            .map(|ix| self.render_tab(ix, cx))
            .collect();
        div()
            .key_context("Workspace")
            .on_action(cx.listener(Self::new_tab))
            .on_action(cx.listener(Self::close_tab))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.terminal_background)
            .font_family(ui_font_family)
            .text_sm()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .h(rems(2.))
                    .bg(theme.tab_bar_background)
                    .border_b_1()
                    .border_color(theme.border)
                    .children(tabs)
                    .child(
                        div()
                            .id("new-tab")
                            .flex()
                            .items_center()
                            .px_3()
                            .text_color(theme.text_muted)
                            .hover(|button| button.text_color(theme.text))
                            .child("+")
                            .on_click(cx.listener(|this, _, window, cx| this.add_tab(window, cx))),
                    ),
            )
            .children(
                self.tabs
                    .get(self.active)
                    .map(|tab| div().flex_1().min_h_0().child(tab.view.clone())),
            )
    }
}
