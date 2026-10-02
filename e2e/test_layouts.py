"""keyboard layouts other than us: typed text, keybindings, dead keys"""

import time

import pytest

from harness import App, Session, session_env, sh

# layouts are switched with setxkbmap, wtype on wayland brings its own keymap every time
pytestmark = pytest.mark.x11_only

FEATURE = "keyboard layouts"


@pytest.fixture
def layout():
    """switch the shared x keyboard layout, set back to us after the test"""

    def switch(name: str, variant: str = "") -> None:
        sh("setxkbmap", "-display", Session.display, name, *(["-variant", variant] if variant else []), env=session_env())

    yield switch
    sh("setxkbmap", "-display", Session.display, "us", env=session_env())


def laid_out(app_factory, keybindings=None) -> App:
    return app_factory({"theme": {"mode": "dark"}, "window_title": ["number"]}, keybindings=keybindings)


def test_de_text_reaches_the_shell(app_factory, layout):
    """on the german layout umlauts, ß and altgr characters are typed as is"""
    layout("de")
    app = laid_out(app_factory)
    text = "zyäöüß@€"
    app.type(f"echo {text} > $T2_TMP/out")
    app.key("Return")
    app.wait_file("out", text)
    app.snap(f"typed {text} on the german layout")


def test_de_keybindings(app_factory, layout):
    """on the german layout ctrl-shift-t still opens a tab, a binding with z follows the z key"""
    layout("de")
    app = laid_out(app_factory, keybindings={"next_tab": "ctrl-shift-z"})
    app.key("ctrl+shift+t")
    app.wait_title("2")
    app.key("alt+1")
    app.wait_title("1")
    # xdotool presses the key that makes z on this layout, where us has y
    app.key("ctrl+shift+z")
    app.wait_title("2")


def test_ru_latin_keybindings(app_factory, layout):
    """on the russian layout the key with latin t still runs ctrl-shift-t"""
    layout("ru")
    app = laid_out(app_factory)
    # the physical t key makes cyrillic ie on this layout
    app.key("ctrl+shift+Cyrillic_ie")
    app.wait_title("2")


def test_dead_keys_compose(app_factory, layout):
    """a dead acute then e types é on us international"""
    layout("us", "intl")
    app = laid_out(app_factory)
    app.type("echo ")
    app.key("dead_acute", "e")
    app.type(" > $T2_TMP/out")
    app.key("Return")
    app.wait_file("out", "é")


def test_fr_alt_digit_follows_keysym(app_factory, layout):
    """keybindings match what a key types, not where it sits: on azerty the 1 key types & unshifted,
    so alt with it does nothing, and alt+shift with it types 1 and picks tab 1"""
    layout("fr")
    app = laid_out(app_factory)
    app.key("ctrl+shift+t")
    app.wait_title("2")
    app.key("alt+ampersand")
    time.sleep(1)
    assert app.title() == "2"
    # xdotool holds shift for the keysym 1 on this layout
    app.key("alt+1")
    app.wait_title("1")
    app.snap("alt+shift with the 1 key on azerty picked tab 1")
