"""tab bar on screen: hiding, tab width, expanded tabs, title align, icons, new tab button, mouse"""

import time

import numpy as np
import pytest

from harness import App, as_rgb, close_to, ink, mask_bbox, near, unique_theme

FEATURE = "tab bar"

THEME = unique_theme(0x60)
C = as_rgb(THEME)
# the title text alone, no icon, so ink in a tab is its title and close button
PLAIN = {"tab_title": [{"text": "title"}], "tab_icon": {"dynamic": False, "default": ""}}


def bar_app(app_factory, **settings) -> App:
    base = {"theme": {"mode": "dark"}, "window_title": ["number"]}
    app = app_factory({**base, **settings}, files={"themes/dark.jsonc": THEME})
    # keep the pointer away from the bar, hover changes its colors
    app.mouse(450, 500)
    return app


def open_tabs(app: App, n: int) -> None:
    """press ctrl+shift+t until there are n tabs, the window title is the active tab number"""
    for i in range(2, n + 1):
        app.key("ctrl+shift+t")
        app.wait_title(str(i))


def bar_shown(app: App, img=None) -> bool:
    img = app.shot() if img is None else img
    return close_to(tuple(img[app.bar_height() - 1, 450]), C["border"])


def borders(app: App, img=None) -> list:
    """x of every border column in the middle row of the bar, the right edge of each tab"""
    img = app.shot() if img is None else img
    row = near(img[app.bar_height() // 2 : app.bar_height() // 2 + 1], C["border"])[0]
    return [int(x) for x in np.flatnonzero(row)]


def muted_ink(app: App, img, x0: int = 0, x1: int = 900) -> tuple | None:
    """bbox of text_muted glyphs in the bar between x0 and x1, like the "+" and inactive titles"""
    h = app.bar_height()
    region = img[1 : h - 2, x0:x1]
    return mask_bbox(near(region, C["text_muted"], 40) & ink(region, C["tab_bar_background"]))


def test_bar_hidden_for_one_tab_by_default(app_factory):
    """hide_bar_for_one_tab hides the bar with one tab, shows it with two, hides it again"""
    app = bar_app(app_factory)
    assert not bar_shown(app)
    app.snap("one tab, no bar")
    open_tabs(app, 2)
    app.wait(lambda: bar_shown(app), msg="bar with two tabs")
    app.snap("two tabs, bar shown")
    app.key("ctrl+shift+w")
    app.wait_title("1")
    app.wait(lambda: not bar_shown(app), msg="bar hidden again")


def test_bar_always_shown_when_not_hiding(app_factory):
    """hide_bar_for_one_tab false keeps the bar with a single tab"""
    app = bar_app(app_factory, hide_bar_for_one_tab=False)
    app.wait(lambda: bar_shown(app), msg="bar")
    img = app.shot()
    h = app.bar_height()
    assert close_to(tuple(img[h // 2, 5]), C["tab_active_background"])
    assert close_to(tuple(img[h // 2, 895]), C["tab_bar_background"])
    assert close_to(tuple(img[h + 2, 450]), C["terminal_background"])


def test_terminal_starts_below_bar(app_factory):
    """the terminal gives the bar its rows, the shell sees fewer lines"""
    app = bar_app(app_factory, hide_bar_for_one_tab=False)
    assert app.pty_size() == app.expected_pty(900, 600, top=app.bar_height())


@pytest.mark.parametrize("width", [120, 220, 260])
def test_fixed_tab_width(app_factory, width):
    """tab_width sets the width of every tab"""
    app = bar_app(app_factory, tab_width=width, **PLAIN)
    open_tabs(app, 3)
    app.wait(lambda: borders(app)[:3] == [width - 1, 2 * width - 1, 3 * width - 1],
             msg=f"tab edges every {width}px, got {borders(app)}")


@pytest.mark.parametrize("tabs", [2, 4])
def test_expanded_tabs_share_the_bar(app_factory, tabs):
    """expand_tabs splits the bar equally, ignoring tab_width"""
    app = bar_app(app_factory, expand_tabs=True, tab_width=50, **PLAIN)
    open_tabs(app, tabs)

    def equal() -> bool:
        edges = borders(app)[:tabs]
        widths = np.diff([-1, *edges])
        return len(edges) == tabs and widths.min() > 100 and widths.max() - widths.min() <= 1

    app.wait(equal, msg=f"equal tabs, edges {borders(app)}")


@pytest.mark.parametrize("align", ["left", "center", "right"])
def test_title_align(app_factory, align):
    """tab_title_align puts the title at the left, center or right of its tab"""
    app = bar_app(app_factory, tab_title_align=align, tab_width=300, **PLAIN)
    open_tabs(app, 2)
    # first tab is inactive, its close button hidden, so its only ink is the title
    app.wait(lambda: muted_ink(app, app.shot(), 0, 299), msg="title of the first tab")
    x0, _, x1, _ = muted_ink(app, app.shot(), 0, 299)
    middle = (x0 + x1) / 2
    expected = {"left": middle < 100, "center": 100 <= middle <= 200, "right": middle > 200}[align]
    assert expected, (align, x0, x1)


@pytest.mark.parametrize("position", ["left", "right"])
def test_tab_icon_position(app_factory, position):
    """tab_icon.position puts the icon before the title, or at the far end of the tab"""
    app = bar_app(app_factory, tab_width=300, tab_title=[{"text": "t"}],
                  tab_icon={"position": position, "dynamic": False, "default": ""})
    open_tabs(app, 2)
    app.wait(lambda: muted_ink(app, app.shot(), 0, 299), msg="icon and title")
    # the title takes the free room, so a right icon is pushed to the far end of the tab
    x0, _, x1, _ = muted_ink(app, app.shot(), 0, 299)
    assert x0 < 40 and (x1 < 100 if position == "left" else x1 > 200), (x0, x1)


@pytest.mark.parametrize("dynamic", [True, False])
def test_dynamic_tab_icon_follows_program(app_factory, dynamic):
    """tab_icon.dynamic shows the icon of the running program, false keeps the default one"""
    app = app_factory({"theme": {"mode": "dark"}, "hide_bar_for_one_tab": False, "tab_title": [{"text": "t"}],
                       "tab_icon": {"dynamic": dynamic, "default": "D"}},
                      files={"themes/dark.jsonc": THEME},
                      tab_icons={"groups": [{"icon": "", "commands": ["sleep"]}]})
    app.mouse(450, 500)
    h = app.bar_height()
    app.wait(lambda: mask_bbox(ink(app.shot()[1 : h - 2, :100], C["tab_active_background"])), msg="icon")
    before = app.shot()[1 : h - 2, :100].copy()
    app.snap("shell running")
    app.run("sleep 30")
    if dynamic:
        app.wait(lambda: (app.shot()[1 : h - 2, :100] != before).any(), msg="icon change")
        app.snap("sleep running, its icon shown")
    else:
        # titles and icons refresh every second
        time.sleep(2.5)
        assert (app.shot()[1 : h - 2, :100] == before).all()


@pytest.mark.parametrize("placement", ["left", "right", "after_tabs"])
def test_new_tab_button(app_factory, placement):
    """new_tab_button sits left, right or after the tabs, a click opens a tab"""
    app = bar_app(app_factory, hide_bar_for_one_tab=False, new_tab_button=placement, tab_width=200, **PLAIN)
    app.wait(lambda: bar_shown(app), msg="bar")
    img = app.shot()
    tab_edges = borders(app, img)
    if placement == "left":
        plus = muted_ink(app, img, 0, 60)
        assert plus is not None and tab_edges[0] > 200, tab_edges
    elif placement == "right":
        plus = muted_ink(app, img, 840, 900)
        assert plus is not None and tab_edges[0] == 199
    else:
        plus = muted_ink(app, img, 200, 260)
        assert plus is not None and tab_edges[0] == 199
    x0, y0, x1, y1 = plus
    base = {"left": 0, "right": 840, "after_tabs": 200}[placement]
    app.click(base + (x0 + x1) // 2, 1 + (y0 + y1) // 2)
    app.wait_title("2")


def test_after_tabs_button_moves_with_tabs(app_factory):
    """the after_tabs button follows the last tab as tabs open"""
    app = bar_app(app_factory, hide_bar_for_one_tab=False, new_tab_button="after_tabs", tab_width=150, **PLAIN)
    open_tabs(app, 3)
    app.mouse(450, 500)
    app.wait(lambda: muted_ink(app, app.shot(), 450, 510), msg="plus after the third tab")


def test_click_tab_activates_it(app_factory):
    """clicking a tab switches to it"""
    app = bar_app(app_factory, tab_width=200, **PLAIN)
    open_tabs(app, 3)
    app.click(100, app.bar_height() // 2)
    app.wait_title("1")
    app.wait(lambda: close_to(tuple(app.shot()[app.bar_height() // 2, 5]), C["tab_active_background"]),
             msg="first tab painted active")


def test_close_button_closes_tab(app_factory):
    """the x on the active tab closes it"""
    app = bar_app(app_factory, tab_width=200, **PLAIN)
    open_tabs(app, 3)
    h = app.bar_height()
    img = app.shot()
    # the active third tab shows its close button at its right end
    region = img[1 : h - 2, 400 + 150 : 400 + 198]
    box = mask_bbox(ink(region, C["tab_active_background"]))
    assert box is not None
    app.click(400 + 150 + (box[0] + box[2]) // 2, 1 + (box[1] + box[3]) // 2)
    app.wait_title("2")


@pytest.mark.parametrize("show", [True, False])
def test_show_tab_close_button(app_factory, show):
    """show_tab_close_button false hides the x on the active and hovered tab, middle click still closes"""
    app = bar_app(app_factory, tab_width=200, show_tab_close_button=show, **PLAIN)
    open_tabs(app, 2)
    h = app.bar_height()
    # hover the first tab too, hovered tabs show their x like the active one
    app.mouse(100, h // 2)
    time.sleep(0.3)
    img = app.shot()
    active = mask_bbox(ink(img[1 : h - 2, 200 + 150 : 200 + 198], C["tab_active_background"]))
    hovered = mask_bbox(ink(img[1 : h - 2, 150:198], C["tab_bar_background"]))
    assert (active is not None) == show and (hovered is not None) == show, (active, hovered)
    app.snap("x shown" if show else "no x, first tab hovered")
    app.click(100, h // 2, button=2)
    app.wait_title("1")


def test_middle_click_closes_tab(app_factory):
    """a middle click on a tab closes it"""
    app = bar_app(app_factory, tab_width=200, **PLAIN)
    open_tabs(app, 3)
    app.click(100, app.bar_height() // 2, button=2)
    app.wait(lambda: len(borders(app)) == 2, msg=f"two tabs left, edges {borders(app)}")


def test_overflowing_tabs_keep_width_and_scroll(app_factory):
    """tabs that do not fit keep their width, the wheel scrolls them into view"""
    app = bar_app(app_factory, tab_width=250, new_tab_button="right", **PLAIN)
    open_tabs(app, 6)
    h = app.bar_height()
    # the active last tab is scrolled into view at the right
    app.wait(lambda: close_to(tuple(app.shot()[h // 2, 800]), C["tab_active_background"]), msg="last tab")
    app.snap("six tabs, scrolled to the active one")
    app.wheel(300, h // 2, up=True, clicks=10)
    app.wait(lambda: not close_to(tuple(app.shot()[h // 2, 800]), C["tab_active_background"]),
             msg="tabs scrolled by the wheel")
