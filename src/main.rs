mod settings;
mod terminal;
mod theme;
mod ui;

use std::{borrow::Cow, sync::Arc};

use gpui::{App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;

use crate::{
    settings::{Keybindings, Settings},
    theme::Theme,
    ui::workspace::Workspace,
};

fn init(cx: &mut App) {
    // 1) bundle jetbrains mono nerd font as binary
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/jetbrains/JetBrainsMonoNLNerdFontMono-Bold.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/jetbrains/JetBrainsMonoNLNerdFontMono-BoldItalic.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/jetbrains/JetBrainsMonoNLNerdFontMono-Italic.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/jetbrains/JetBrainsMonoNLNerdFontMono-Regular.ttf"
        )),
    ];
    cx.text_system()
        .add_fonts(fonts)
        .expect("failed to load bundled fonts");

    // 2) load settings
    cx.set_global(Settings::load());

    // 3) apply theme
    Theme::apply(cx.window_appearance(), cx);

    // 4) load keybindings
    cx.bind_keys(Keybindings::load().bindings());
}

fn main() {
    application().run(|cx: &mut App| {
        init(cx);
        cx.on_window_closed(|cx, _| cx.quit()).detach();

        let bounds = Bounds::centered(None, size(px(900.), px(600.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(400.), px(250.))),
                titlebar: Some(TitlebarOptions {
                    title: Some(Settings::get(cx).default_title.clone().into()),
                    ..Default::default()
                }),
                app_id: Some("gpuz_term".into()),
                icon: Some(Arc::new(
                    image::load_from_memory(include_bytes!("../assets/logo/icon_256.png"))
                        .expect("failed to load bundled icon")
                        .into_rgba8(),
                )),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Workspace::new(window, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
