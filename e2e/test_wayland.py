"""wayland only: app id, fractional output scale, clipboard through wl-clipboard"""

import pytest

from harness import Input, get_clipboard, set_clipboard, swaymsg
from test_font import SAMPLE, check_cells

FEATURE = "wayland"

pytestmark = pytest.mark.wayland_only


@pytest.fixture
def output_scale():
    """set the output scale for one test, back to 1 after it"""
    name = Input.output()["name"]
    yield lambda scale: swaymsg(f"output {name} scale {scale}")
    swaymsg(f"output {name} scale 1")


def test_app_id_is_kuterm(app_factory):
    """the xdg app id is kuterm, for docks and window rules"""
    app = app_factory({"theme": {"mode": "dark"}})
    assert app._node()["app_id"] == "kuterm"


@pytest.mark.parametrize("scale", ["1.5", "2"])
def test_fractional_output_scale(app_factory, output_scale, scale):
    """on a scaled output the cells scale and the pty keeps its size"""
    output_scale(scale)
    app = app_factory({"theme": {"mode": "dark"}}, script=SAMPLE)
    assert app.scale == float(scale)
    assert app.size() == (round(900 * app.scale), round(600 * app.scale))
    check_cells(app)
    app.snap(f"sample text on an output scaled {scale}")
    app.close()
    bash = app_factory({"theme": {"mode": "dark"}})
    assert bash.pty_size() == bash.expected_pty(900, 600)


def test_clipboard_round_trip(app_factory):
    """wl-copy text pastes into the shell, a selection copies back out to wl-paste"""
    app = app_factory({"theme": {"mode": "dark"}})
    set_clipboard(f"echo from-wl-copy > {app.file('out')}\n")
    app.key("ctrl+shift+v")
    app.key("Return")
    app.wait_file("out", "from-wl-copy")
    app.run("clear; echo copy-me-out")
    x0, y0, x1, y1 = app.cell_rect(0, 0, 11)
    app.wait(lambda: app.shot()[y0:y1, x0:x1].std() > 10, msg="text shown")
    app.drag((x0 + 1, (y0 + y1) // 2), (x1 - 1, (y0 + y1) // 2))
    app.key("ctrl+shift+c")
    app.wait(lambda: get_clipboard() == "copy-me-out", msg=f"clipboard, got {get_clipboard()!r}")
