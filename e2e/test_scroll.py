"""scrollback on screen: scrollbar modes, placement, width, auto hide, history length, smooth scroll"""

import time

import numpy as np
import pytest

from harness import App, bundled_theme, mask_bbox, near

FEATURE = "scrollbar and scrolling"

DARK = bundled_theme(True)
# no glyph edge blends into pure magenta, so its pixels are the thumb alone
BAR = (255, 0, 255)
LINES = "for i in $(seq 1 {n}); do echo line $i; done\n"


def scroll_app(app_factory, scrollbar=None, lines: int = 0, **terminal) -> App:
    # auto hide keeps even an "on" bar hidden until the first scroll, so it is off unless tested
    terminal["scrollbar"] = {"auto_hide": 0, **(scrollbar or {})}
    return app_factory({"theme": {"mode": "dark"}, "terminal": terminal},
                       script=LINES.format(n=lines) if lines else None,
                       files={"themes/dark.jsonc": {"scrollbar": "#ff00ff"}})


def thumb(app: App, img=None) -> tuple | None:
    """bbox of the scrollbar thumb"""
    img = app.shot() if img is None else img
    return mask_bbox(near(img, BAR, 3))


@pytest.mark.parametrize("enable,history,shown", [
    ("dynamic", False, False), ("dynamic", True, True),
    ("on", False, True), ("on", True, True),
    ("off", False, False), ("off", True, False),
])
def test_scrollbar_enable(app_factory, enable, history, shown):
    """scrollbar.enable on always shows it, off never, dynamic once there is history"""
    app = scroll_app(app_factory, {"enable": enable}, lines=200 if history else 0)
    if history:
        app.wait(lambda: near(app.shot(), DARK["terminal_foreground"], 20).sum() > 500, msg="output")
    if shown:
        app.wait(lambda: thumb(app), msg="scrollbar")
        x0, _, x1, _ = thumb(app)
        assert x1 == 900 and x1 - x0 == 8
    else:
        time.sleep(1)
        assert thumb(app) is None


def test_scrollbar_off_gives_room_to_the_grid(app_factory):
    """with the scrollbar off its width goes to the grid"""
    app = scroll_app(app_factory, {"enable": "off"}, font_size=10)
    assert app.pty_size() == app.expected_pty(900, 600)


@pytest.mark.parametrize("placement", ["left", "right"])
def test_scrollbar_placement(app_factory, placement):
    """scrollbar.placement puts the bar at the left or right edge, the grid moves over"""
    app = scroll_app(app_factory, {"enable": "on", "placement": placement, "width": 12})
    app.wait(lambda: thumb(app), msg="scrollbar")
    x0, _, x1, _ = thumb(app)
    assert (x0, x1) == ((0, 12) if placement == "left" else (888, 900))


@pytest.mark.parametrize("width,expected", [(2, 2), (30, 30), (100, 64), (1, 8)])
def test_scrollbar_width(app_factory, width, expected):
    """scrollbar.width sets the bar width, limited to 2..64"""
    app = scroll_app(app_factory, {"enable": "on", "width": width})
    app.wait(lambda: thumb(app), msg="scrollbar")
    x0, _, x1, _ = thumb(app)
    assert x1 - x0 == expected


def test_scrollbar_auto_hide(app_factory):
    """scrollbar.auto_hide hides the bar after some quiet seconds, scrolling shows it again"""
    app = scroll_app(app_factory, {"enable": "dynamic", "auto_hide": 1}, lines=200)
    app.wait(lambda: thumb(app), msg="scrollbar")
    app.snap("history printed, bar shown")
    app.wait(lambda: thumb(app) is None, msg="bar hidden", timeout=5)
    app.snap("bar hidden after a second")
    app.wheel(450, 300, up=True, clicks=3)
    app.wait(lambda: thumb(app), msg="bar back after scrolling")


@pytest.mark.parametrize("history", [50, 0])
def test_max_history_length(app_factory, history):
    """max_history_length limits the scrollback, the thumb shows how much is kept"""
    app = scroll_app(app_factory, {"enable": "on"}, lines=1000, max_history_length=history)
    rows = app.expected_pty(900, 600)[0]
    kept = 50 if history else 1000 - rows + 1
    expected = max(600 * rows / (kept + rows), 20)
    app.wait(lambda: thumb(app) and abs((thumb(app)[3] - thumb(app)[1]) - expected) <= 3,
             msg=f"thumb {expected:.0f}px high, got {thumb(app)}")


def test_click_on_track_jumps(app_factory):
    """clicking the scrollbar track jumps there, the thumb follows"""
    app = scroll_app(app_factory, {"enable": "on"}, lines=500, smooth_scroll={"enable": False})
    app.wait(lambda: thumb(app) and thumb(app)[3] >= 598, msg=f"thumb at the bottom, got {thumb(app)}")
    app.click(895, 5)
    app.wait(lambda: thumb(app) and thumb(app)[1] <= 2, msg=f"thumb at the top, got {thumb(app)}")


@pytest.mark.parametrize("enable", [True, False])
def test_smooth_scroll(app_factory, enable):
    """smooth_scroll glides to the new position over its duration, disabled jumps right away"""
    app = scroll_app(app_factory, {"enable": "on"}, lines=500,
                     smooth_scroll={"enable": enable, "duration": 1000, "easing": "linear"})
    app.wait(lambda: thumb(app) and thumb(app)[3] >= 598, msg=f"thumb at the bottom, got {thumb(app)}")
    time.sleep(0.5)
    app.wheel(450, 300, up=True, clicks=5)
    frames = []
    start = time.monotonic()
    while time.monotonic() - start < 1.6:
        frames.append(thumb(app))
        time.sleep(0.05)
    tops = [f[1] for f in frames if f]
    final = tops[-1]
    # positions the thumb went through on its way
    between = {t for t in tops if t not in (tops[0], final)}
    if enable:
        assert len(set(tops)) >= 4, tops
    else:
        assert len(between) <= 1, tops
    assert final < 600 - 30
    assert np.all(np.diff(tops) <= 0) or not enable
