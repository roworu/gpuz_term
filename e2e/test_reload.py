"""config reload from the palette: settings, themes, keybindings and all configs, without a restart"""

import time

from harness import App, bundled_theme, close_to, dominant

FEATURE = "config reload"

DARK = bundled_theme(True)
LIGHT = bundled_theme(False)


def reload_app(app_factory, **settings) -> App:
    app = app_factory({"theme": {"mode": "dark"}, "window_title": ["number"], **settings})
    app.mouse(100, 100)
    return app


def bg(app: App) -> tuple:
    return dominant(app.shot())


def test_reload_settings(app_factory):
    """"reload settings" applies an edited settings file and shows a notification"""
    app = reload_app(app_factory)
    app.wait(lambda: close_to(bg(app), DARK["terminal_background"]), msg="dark")
    app.write("settings.jsonc", {**app.settings, "theme": {"mode": "light"}, "window_title": [{"text": "reloaded"}]})
    app.palette("config: reload settings")
    app.wait(lambda: close_to(bg(app), LIGHT["terminal_background"]), msg="light after reload")
    app.wait_title("reloaded")
    app.snap("settings reloaded")


def test_reload_themes(app_factory):
    """"reload themes" reads the theme files again"""
    app = reload_app(app_factory)
    app.wait(lambda: close_to(bg(app), DARK["terminal_background"]), msg="dark")
    app.write("themes/dark.jsonc", {"terminal_background": "#204060"})
    app.palette("config: reload themes")
    app.wait(lambda: close_to(bg(app), (0x20, 0x40, 0x60)), msg="new background")
    app.snap("themes reloaded")


def test_reload_keybindings(app_factory):
    """"reload keybindings" moves actions to the keys in the edited file"""
    app = reload_app(app_factory)
    app.write("keybindings.jsonc", {"new_tab": "ctrl-shift-y"})
    app.palette("config: reload keybindings")
    time.sleep(0.5)
    app.key("ctrl+shift+y")
    app.wait_title("2")
    app.key("ctrl+shift+t")
    time.sleep(1.5)
    assert app.title() == "2"


def test_reload_all(app_factory):
    """"reload all configs" also reads commands.jsonc again"""
    app = reload_app(app_factory)
    app.write("commands.jsonc", {"commands": [
        {"name": "brand new", "actions": ["new_tab"]}, {"name": "reload everything", "actions": ["reload_all"]}]})
    app.write("themes/dark.jsonc", {"terminal_background": "#402010"})
    app.palette("config: reload all configs")
    app.wait(lambda: close_to(bg(app), (0x40, 0x20, 0x10)), msg="new background")
    app.palette("brand new")
    app.wait_title("2")
