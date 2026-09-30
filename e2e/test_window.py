"""the real window: initial size, minimum size enforced by the window manager, resizing"""

import pytest

from harness import BACKEND, App, bundled_theme, close_to, dominant, sh, session_env

FEATURE = "window"

DARK = bundled_theme(True)


def dark_app(app_factory, **kwargs) -> App:
    return app_factory({"theme": {"mode": "dark"}}, **kwargs)


def test_initial_size(app_factory):
    """the window opens at 900x600 with the terminal filling it"""
    app = dark_app(app_factory)
    assert app.size() == (900, 600)
    assert close_to(dominant(app.shot()), DARK["terminal_background"])


@pytest.mark.x11_only
def test_min_size_hint_is_set(app_factory):
    """the window manager gets a 400x250 minimum size hint"""
    app = dark_app(app_factory)
    hints = sh("xprop", "-id", app.wid, "WM_NORMAL_HINTS", env=session_env())
    assert "minimum size: 400 by 250" in hints, hints


@pytest.mark.parametrize("w,h", [(399, 600), (900, 249), (1, 1)])
def test_window_cannot_shrink_below_min_size(app_factory, w, h):
    """asking for a smaller window stops at the minimum size and still paints everything.
    sway's resize command does not apply client minimums, so there only painting is checked"""
    app = dark_app(app_factory)
    app.resize(w, h)
    app.wait(lambda: app.size() != (900, 600), msg="the resize to happen", timeout=5)
    got = app.size()
    if BACKEND == "x11":
        assert got[0] >= 400 and got[1] >= 250, got
    assert app.alive()
    app.wait(lambda: close_to(dominant(app.shot()), DARK["terminal_background"]), msg="painted after resize")


@pytest.mark.parametrize("w,h", [(400, 250), (640, 480), (1300, 900)])
def test_resize_updates_pty(app_factory, w, h):
    """the shell sees the new rows and columns after a resize"""
    app = dark_app(app_factory)
    app.wait(lambda: app.pty_size("before") == app.expected_pty(900, 600), msg="initial pty size")
    app.resize(w, h)
    app.wait(lambda: app.size() == (w, h), msg=f"window {w}x{h}, got {app.size()}")
    expected = app.expected_pty(w, h)
    app.wait(lambda: app.pty_size(f"after{w}") == expected, msg=f"pty {expected}")
    img = app.shot()
    # the bottom corner is terminal background, not unpainted black. the right one may hold the scrollbar
    assert close_to(tuple(img[-3, 3]), DARK["terminal_background"])


def test_content_survives_resize(app_factory):
    """text printed before a resize is still on screen after it"""
    app = dark_app(app_factory, script="printf '\\033[41m%s\\033[0m\\n' '          '")
    red = DARK["ansi"][1]
    app.wait(lambda: close_to(app.cells_color(0, 0, 10), red), msg="red block")
    app.resize(1200, 800)
    app.wait(lambda: app.size() == (1200, 800), msg="resize")
    app.wait(lambda: close_to(app.cells_color(0, 0, 10), red), msg="red block after resize")
