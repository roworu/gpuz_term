mod settings;
mod terminal;
mod theme;
mod ui;

use std::borrow::Cow;

use gpui::{
    App, AppContext, Bounds, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use gpui_platform::application;

use crate::{
    settings::Settings,
    ui::{
        terminal_view::Paste,
        workspace::{CloseTab, NewTab, Workspace},
    },
};

fn load_fonts(cx: &App) {
    // bundle jetbrains mono nerd font as binary
    let fonts: Vec<Cow<'static, [u8]>> = vec![

      Cow::Borrowed(include_bytes!("../assets/fonts/jetbrains/JetBrainsMonoNLNerdFontMono-Bold.ttf")),
      Cow::Borrowed(include_bytes!("../assets/fonts/jetbrains/JetBrainsMonoNLNerdFontMono-BoldItalic.ttf")),
      Cow::Borrowed(include_bytes!("../assets/fonts/jetbrains/JetBrainsMonoNLNerdFontMono-Italic.ttf")),
      Cow::Borrowed(include_bytes!("../assets/fonts/jetbrains/JetBrainsMonoNLNerdFontMono-Regular.ttf")),
    
    ];
    cx.text_system()
        .add_fonts(fonts)
        .expect("failed to load bundled fonts");
}

fn main() {
    
    application().run(|cx: &mut App| {
        load_fonts(cx);
        cx.set_global(Settings::load());
        cx.bind_keys([
            KeyBinding::new("ctrl-t", NewTab, None),
            KeyBinding::new("ctrl-w", CloseTab, None),
            KeyBinding::new("ctrl-shift-v", Paste, Some("Terminal")),
        ]);
        cx.on_window_closed(|cx, _| cx.quit()).detach();

        let bounds = Bounds::centered(None, size(px(900.), px(600.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("gpuz_term".into()),
                    ..Default::default()
                }),
                app_id: Some("gpuz_term".into()),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Workspace::new(window, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
