"""close tab dialog: closing a tab with a running program asks first, keys and clicks answer it"""

import time

import pytest

from harness import App, as_rgb, count, ink, mask_bbox, near, unique_theme

FEATURE = "close tab dialog"

THEME = unique_theme(0x70)
C = as_rgb(THEME)


def busy_app(app_factory, program: str = "sleep 1000", **settings) -> App:
    """two tabs, the active second one running program in the foreground"""
    app = app_factory({"theme": {"mode": "dark"}, "window_title": ["number", {"text": " "}, "command"],
                       "tab_width": 200, "tab_title": [{"text": "title"}], **settings},
                      files={"themes/dark.jsonc": THEME})
    app.mouse(450, 580)
    app.wait_title("1 bash")
    app.key("ctrl+shift+t")
    app.wait_title("2 bash")
    app.run(program)
    app.wait_title(f"2 {program.split()[0]}")
    return app


def dialog(app: App) -> tuple | None:
    """bbox of the "close" button, drawn in the danger color, none while the dialog is closed"""
    mask = near(app.shot(), C["danger_button"])
    return mask_bbox(mask) if mask.sum() > 200 else None


def cancel_selected(app: App) -> bool:
    """the "cancel" button is filled like the active tab once it is the one enter presses"""
    img = app.shot()[app.bar_height() + 10 :]
    return count(img, C["tab_active_background"]) > 200


def ask(app: App) -> tuple:
    """press the close tab key and wait for the dialog, returns the "close" button bbox"""
    app.key("ctrl+shift+w")
    return app.wait(lambda: dialog(app), msg="close tab dialog")


def kept(app: App) -> None:
    """the dialog closed and the busy tab is still open"""
    app.wait(lambda: dialog(app) is None, msg="dialog closed")
    time.sleep(0.5)
    assert app.title() == "2 sleep"


def test_running_tab_asks_before_closing(app_factory):
    """closing a tab with a running program shows the dialog, escape keeps the tab, enter closes it"""
    app = busy_app(app_factory)
    x0, y0, x1, _ = ask(app)
    # centered near the top like the palette, "close" at the right end of the panel
    assert x1 > 450 and 60 <= y0 <= 200, (x0, y0, x1)
    assert not cancel_selected(app)
    app.snap("dialog asking to close the tab running sleep")
    app.key("Escape")
    kept(app)
    app.snap("escape kept the tab")
    ask(app)
    app.key("Return")
    app.wait_title("1 bash")
    assert dialog(app) is None


def test_arrows_pick_the_button_enter_presses(app_factory):
    """left selects "cancel", right selects "close" again"""
    app = busy_app(app_factory)
    ask(app)
    app.key("Left")
    app.wait(lambda: cancel_selected(app), msg="cancel selected")
    app.snap("cancel selected")
    app.key("Return")
    kept(app)
    ask(app)
    app.key("Left", "Right")
    app.wait(lambda: not cancel_selected(app), msg="close selected")
    app.key("Return")
    app.wait_title("1 bash")


@pytest.mark.parametrize("key, closes", [("y", True), ("n", False)])
def test_y_and_n_answer(app_factory, key, closes):
    """y closes the tab, n keeps it"""
    app = busy_app(app_factory)
    ask(app)
    app.key(key)
    app.wait(lambda: dialog(app) is None, msg="dialog closed")
    if closes:
        app.wait_title("1 bash")
    else:
        time.sleep(0.5)
        assert app.title() == "2 sleep"


def test_click_on_close_button(app_factory):
    """clicking "close" closes the tab"""
    app = busy_app(app_factory)
    x0, y0, x1, y1 = ask(app)
    app.click((x0 + x1) // 2, (y0 + y1) // 2)
    app.wait_title("1 bash")


def test_click_outside_cancels(app_factory):
    """a click outside the dialog keeps the tab"""
    app = busy_app(app_factory)
    ask(app)
    app.click(450, 550)
    kept(app)


@pytest.mark.parametrize("button", [1, 2])
def test_tab_bar_closing_asks_too(app_factory, button):
    """the x of the tab and a middle click on it ask like the key does"""
    app = busy_app(app_factory)
    h = app.bar_height()
    if button == 1:
        # the active second tab shows its x at its right end
        box = mask_bbox(ink(app.shot()[1 : h - 2, 350:398], C["tab_active_background"]))
        assert box is not None
        app.click(350 + (box[0] + box[2]) // 2, 1 + (box[1] + box[3]) // 2)
    else:
        app.click(300, h // 2, button=2)
    app.wait(lambda: dialog(app), msg="close tab dialog")
    app.snap("asked after a click on the tab bar")
    app.key("Return")
    app.wait_title("1 bash")


def test_keys_do_not_reach_the_program(app_factory):
    """keys typed while the dialog is open are not sent to the tab behind it"""
    app = busy_app(app_factory, program="cat > $T2_TMP/typed")
    ask(app)
    app.type("abc")
    app.snap("typed abc while asked")
    app.key("Escape")
    app.wait(lambda: dialog(app) is None, msg="dialog closed")
    # the first ctrl-d sends a pending line to cat, the second one ends it
    app.key("ctrl+d", "ctrl+d")
    app.wait_title("2 bash")
    assert app.file("typed").read_text() == ""


def test_dialog_closes_when_the_program_tab_exits(app_factory):
    """a tab whose shell exits while asked about closes, and the dialog with it"""
    app = busy_app(app_factory, program="sleep 4; exit")
    ask(app)
    app.wait_title("1 bash")
    app.wait(lambda: dialog(app) is None, msg="dialog closed with its tab")


def test_disabled_warning_closes_at_once(app_factory):
    """close_running_tab_warn false closes a busy tab without asking"""
    app = busy_app(app_factory, close_running_tab_warn=False)
    app.key("ctrl+shift+w")
    app.wait_title("1 bash")
    assert dialog(app) is None
