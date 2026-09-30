"""e2e helpers: start the real kuterm on the xvfb display, drive it with xdotool and read pixels.

the display, window manager and session bus are started once per test session in conftest.py.
every App gets its own temp XDG_CONFIG_HOME and HOME, so cases never share config files.
"""

import contextlib
import json
import math
import os
import re
import shutil
import signal
import subprocess
import tempfile
import time
from pathlib import Path

import numpy as np
from PIL import ImageGrab

SRC = Path(os.environ.get("KUTERM_SRC", "/src"))
BIN = Path(os.environ.get("KUTERM_BIN", SRC / "target/podman/debug/kuterm"))
ASSETS = SRC / "assets"
ARTIFACTS = Path(os.environ.get("KUTERM_ARTIFACTS", SRC / "e2e/artifacts"))

# bundled jetbrains mono advance is 600 of 1000 units
JB_ADVANCE = 0.6

# the pty starts at a tiny default size until the first layout, output printed before the
# resize can get shifted, so scripts wait for the real size first
SCRIPT_PREAMBLE = (
    'i=0; while [ "$(stty size)" = "6 100" ] && [ $i -lt 200 ]; do sleep 0.05; i=$((i+1)); done; sleep 0.1\n'
    # step N waits until the test calls app.step(N), to change the screen between checks
    'step() { while [ ! -f "$T2_TMP/step$1" ]; do sleep 0.02; done; }\n'
)

# interactive bash without user files, so the prompt is just "$ "
BASH = {"with_arguments": {"program": "/bin/bash", "args": ["--norc", "--noprofile", "-i"]}}


def jsonc(path: Path) -> dict:
    """parse a bundled jsonc file (full line comments and trailing commas)"""
    text = "\n".join(line for line in path.read_text().splitlines() if not line.strip().startswith("//"))
    text = re.sub(r",(\s*[}\]])", r"\1", text)
    return json.loads(text)


def rgb(value: str) -> tuple:
    """'#rrggbb' or '#rgb' to an (r, g, b) tuple"""
    value = value.lstrip("#")
    if len(value) in (3, 4):
        value = "".join(c * 2 for c in value[:3])
    return tuple(int(value[i : i + 2], 16) for i in (0, 2, 4))


def as_rgb(theme: dict) -> dict:
    """theme colors as rgb tuples, ansi and ansi_dim as lists"""
    return {k: [rgb(c) for c in v] if isinstance(v, list) else rgb(v) for k, v in theme.items()}


def bundled_theme(dark: bool) -> dict:
    return as_rgb(jsonc(ASSETS / ("default_theme_dark.jsonc" if dark else "default_theme_light.jsonc")))


def bundled_settings() -> dict:
    return jsonc(ASSETS / "default_settings.jsonc")


def unique_theme(seed: int) -> dict:
    """a full theme where every color differs from each other and from both bundled themes"""
    colors = iter(f"#{seed:02x}{i * 37 % 256:02x}{(i * 91 + 50) % 256:02x}" for i in range(1, 60))
    keys = ["tab_bar_background", "tab_active_background", "border", "text", "text_muted",
            "terminal_background", "terminal_foreground", "cursor", "selection", "scrollbar",
            "bright_foreground", "dim_foreground"]
    theme = {key: next(colors) for key in keys}
    theme["ansi"] = [next(colors) for _ in range(16)]
    theme["ansi_dim"] = [next(colors) for _ in range(8)]
    return theme


def wait_until(fn, timeout: float = 10.0, interval: float = 0.1, msg: str = "condition"):
    """poll fn until it returns something truthy, fail with its last value on timeout"""
    deadline = time.monotonic() + timeout
    last = None
    while True:
        try:
            last = fn()
        except Exception as error:  # a check may race the app, retry until the deadline
            last = error
        # numpy arrays have no truth value, any array counts as found
        if not isinstance(last, Exception) and (isinstance(last, np.ndarray) or last):
            return last
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out after {timeout}s waiting for {msg}, last value: {last!r}")
        time.sleep(interval)


def sh(*args: str, check: bool = True, timeout: float = 10, **kw) -> str:
    """run a command and return stdout"""
    res = subprocess.run(args, capture_output=True, text=True, timeout=timeout, **kw)
    if check and res.returncode != 0:
        raise RuntimeError(f"{args} failed ({res.returncode}): {res.stderr.strip()}")
    return res.stdout


def diff(img: np.ndarray, color: tuple) -> np.ndarray:
    """per pixel max channel distance to color"""
    return np.abs(img.astype(np.int16) - np.array(color[:3], dtype=np.int16)).max(axis=-1)


def near(img: np.ndarray, color: tuple, tol: int = 2) -> np.ndarray:
    """bool mask of pixels within tol of color on every channel"""
    return diff(img, color) <= tol


def count(img: np.ndarray, color: tuple, tol: int = 2) -> int:
    return int(near(img, color, tol).sum())


def mask_bbox(mask: np.ndarray) -> tuple | None:
    ys, xs = np.nonzero(mask)
    if len(xs) == 0:
        return None
    return int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1


def median(img: np.ndarray) -> tuple:
    """median color of a region"""
    return tuple(int(v) for v in np.median(img.reshape(-1, 3), axis=0))


def dominant(img: np.ndarray) -> tuple:
    """most common exact color of a region"""
    flat = img.reshape(-1, 3).astype(np.int32)
    keys = (flat[:, 0] << 16) | (flat[:, 1] << 8) | flat[:, 2]
    values, counts = np.unique(keys, return_counts=True)
    k = int(values[counts.argmax()])
    return (k >> 16) & 255, (k >> 8) & 255, k & 255


def close_to(a: tuple, b: tuple, tol: int = 2) -> bool:
    return all(abs(int(x) - int(y)) <= tol for x, y in zip(a[:3], b[:3]))


def ink(img: np.ndarray, background: tuple, threshold: int = 40) -> np.ndarray:
    """bool mask of pixels that differ clearly from the background"""
    return diff(img, background) > threshold


def ink_bbox(img: np.ndarray, background: tuple, threshold: int = 40) -> tuple | None:
    """bbox of pixels that differ clearly from the background, e.g. antialiased text"""
    return mask_bbox(ink(img, background, threshold))


class Session:
    display = os.environ.get("DISPLAY", ":99")
    runtime_dir = "/tmp/e2e-runtime"
    bus_address = ""


def x_env() -> dict:
    env = {
        "PATH": os.environ.get("PATH", "/usr/local/bin:/usr/bin:/bin"),
        "DISPLAY": Session.display,
        "WAYLAND_DISPLAY": "",
        "XDG_RUNTIME_DIR": Session.runtime_dir,
        "GPUI_X11_SCALE_FACTOR": "1",
        "LANG": "C.UTF-8",
        "USER": "root",
        "TERM": "dumb",
    }
    if Session.bus_address:
        env["DBUS_SESSION_BUS_ADDRESS"] = Session.bus_address
    if "VK_ICD_FILENAMES" in os.environ:
        env["VK_ICD_FILENAMES"] = os.environ["VK_ICD_FILENAMES"]
    return env


def xdo(*args: str, check: bool = True) -> str:
    return sh("xdotool", *args, check=check, env=x_env())


def screen(x: int, y: int, w: int, h: int) -> np.ndarray:
    """grab a screen area as an rgb array"""
    img = ImageGrab.grab(bbox=(x, y, x + w, y + h), xdisplay=Session.display)
    return np.asarray(img.convert("RGB"))


def set_clipboard(text: str) -> None:
    """own the clipboard with xclip, it keeps serving until someone else takes it"""
    proc = subprocess.Popen(["xclip", "-selection", "clipboard", "-i"], stdin=subprocess.PIPE,
                            env=x_env(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    proc.communicate(text.encode(), timeout=5)


def get_clipboard() -> str:
    return sh("xclip", "-selection", "clipboard", "-o", check=False, env=x_env())


class Portal:
    """fake desktop portal answering color-scheme, 0 none, 1 dark, 2 light"""

    def __init__(self, scheme: int) -> None:
        self.proc = subprocess.Popen(
            ["python3", str(Path(__file__).with_name("fake_portal.py")), str(scheme)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            env={**x_env(), "PYTHONDONTWRITEBYTECODE": "1"},
        )
        line = self.proc.stdout.readline().strip()
        assert line == "ready", f"fake portal failed to start: {line!r}"

    def set(self, scheme: int) -> None:
        self.proc.stdin.write(f"{scheme}\n")
        self.proc.stdin.flush()
        assert self.proc.stdout.readline().strip() == "ok"

    def stop(self) -> None:
        if self.proc.poll() is None:
            self.proc.stdin.close()
            try:
                self.proc.wait(5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()


class App:
    """one kuterm process with its own config folder, closed with close().

    settings: dict written as settings.jsonc (the app merges it over defaults), or a raw string.
    script: sh script run by the default profile, it ends with a long sleep so tabs stay open.
    without script and without profiles in settings, an interactive bash --norc with PS1="$ " runs.
    commands, keybindings, tab_icons: dicts or raw strings written to their config files.
    files: extra files inside the config folder, like {"themes/dark.jsonc": {...}}.
    args: command line options.
    on_shot: called with (app, caption, png path) for every screenshot taken for the report.
    """

    def __init__(
        self,
        settings=None,
        *,
        script: str | None = None,
        keybindings=None,
        commands=None,
        tab_icons=None,
        files: dict | None = None,
        args: list | None = None,
        env: dict | None = None,
        name: str = "app",
        start: bool = True,
        on_shot=None,
    ) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix=f"e2e-{name}-"))
        self.home = self.tmp / "home"
        self.config_home = self.tmp / "config"
        self.config = self.config_home / "kuterm"
        self.home.mkdir()
        self.config.mkdir(parents=True)
        self.settings = dict(settings) if isinstance(settings, dict) else settings
        self.args = [str(a) for a in (args or [])]
        self.env_extra = env or {}
        self.proc = None
        self.wid = None
        self.log_path = self.tmp / "app.log"
        self.on_shot = on_shot
        # config files as written, shown in the report as the tested state
        self.written: dict = {}

        if isinstance(self.settings, dict) and "profiles" not in self.settings:
            command = BASH
            if script is not None:
                script_path = self.tmp / "run.sh"
                script_path.write_text(SCRIPT_PREAMBLE + script + "\nexec sleep 100000\n")
                command = {"with_arguments": {"program": "/bin/sh", "args": [str(script_path)]}}
            self.settings["profiles"] = [{"name": "test", "command": command}]
        if self.settings is not None:
            self.write("settings.jsonc", self.settings)
        for rel, content in (("keybindings.jsonc", keybindings), ("commands.jsonc", commands),
                             ("tab_icons.jsonc", tab_icons), *(files or {}).items()):
            if content is not None:
                self.write(rel, content)
        if start:
            self.start()

    def write(self, rel: str, content) -> Path:
        """write a config file relative to the config folder, or an absolute path"""
        path = self.config / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        text = content if isinstance(content, str) else json.dumps(content, indent=2, ensure_ascii=False)
        path.write_text(text)
        self.written[rel] = text
        return path

    def app_env(self) -> dict:
        env = x_env()
        env.update({"HOME": str(self.home), "XDG_CONFIG_HOME": str(self.config_home), "PS1": "$ ",
                    "T2_TMP": str(self.tmp)})
        for key, value in self.env_extra.items():
            if value is None:
                env.pop(key, None)
            else:
                env[key] = value
        return env

    def start(self) -> None:
        self.log = open(self.log_path, "w")
        self.proc = subprocess.Popen(
            [str(BIN), *self.args], cwd=self.home, env=self.app_env(), stdout=self.log, stderr=subprocess.STDOUT
        )
        self.wid = wait_until(self._find_window, timeout=30, msg="the app window")
        # give it focus like a user clicking it, openbox usually does this on map already
        xdo("windowactivate", "--sync", self.wid, check=False)
        # first frame: the window is no longer blank black
        wait_until(self._painted, timeout=30, msg="the first painted frame")

    def _painted(self) -> bool:
        if not self.alive():
            raise AssertionError(f"app exited with {self.proc.returncode}:\n{self.output()}")
        img = self.shot()
        return count(img, (0, 0, 0), 0) < img.shape[0] * img.shape[1] // 2

    def _find_window(self):
        if self.proc.poll() is not None:
            raise AssertionError(f"app exited early with {self.proc.returncode}:\n{self.output()}")
        out = xdo("search", "--onlyvisible", "--pid", str(self.proc.pid), check=False).split()
        return out[0] if out else None

    def alive(self) -> bool:
        return self.proc is not None and self.proc.poll() is None

    def output(self) -> str:
        """everything the app printed to stdout and stderr so far"""
        with contextlib.suppress(Exception):
            self.log.flush()
        return self.log_path.read_text(errors="replace") if self.log_path.exists() else ""

    def close(self) -> None:
        if self.proc is not None and self.proc.poll() is None:
            self.proc.send_signal(signal.SIGTERM)
            try:
                self.proc.wait(5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        # shells and sleeps started by the app are its children, don't leak them into later tests
        sh("pkill", "-KILL", "-f", str(self.tmp), check=False)
        with contextlib.suppress(Exception):
            self.log.close()

    def cleanup(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def geometry(self) -> tuple:
        """(x, y, w, h) of the client area on screen"""
        info = sh("xwininfo", "-id", self.wid, env=x_env())
        val = dict(line.strip().split(":", 1) for line in info.splitlines() if ":" in line)
        return tuple(int(val[k]) for k in ("Absolute upper-left X", "Absolute upper-left Y", "Width", "Height"))

    def size(self) -> tuple:
        return self.geometry()[2:]

    def shot(self) -> np.ndarray:
        """screenshot of the client area"""
        return screen(*self.geometry())

    def snap(self, caption: str) -> None:
        """screenshot for the report, showing the state reached so far"""
        if self.on_shot is not None and self.alive():
            self.on_shot(self, caption)

    def wait(self, check, msg: str = "condition", timeout: float = 10.0):
        """wait_until for this app, adding the app output when it times out"""
        try:
            return wait_until(check, timeout=timeout, msg=msg)
        except AssertionError as error:
            raise AssertionError(f"{error}\napp output:\n{self.output()[-1500:]}") from None

    def title(self) -> str:
        return xdo("getwindowname", self.wid).rstrip("\n")

    def wait_title(self, expected, timeout: float = 10) -> str:
        """wait for the window title to equal expected (or satisfy it, when callable)"""
        matches = expected if callable(expected) else (lambda t: t == expected)

        def check() -> str | None:
            title = self.title()
            return title if matches(title) else None

        return self.wait(check, timeout=timeout, msg=f"window title {expected!r} (now {self.title()!r})")

    def focus(self) -> None:
        xdo("windowactivate", "--sync", self.wid, check=False)
        wait_until(lambda: xdo("getactivewindow", check=False).strip() == self.wid, timeout=5, msg="focus")

    def resize(self, w: int, h: int) -> None:
        xdo("windowsize", "--sync", self.wid, str(w), str(h), check=False)

    def key(self, *keys: str) -> None:
        """press keys with xdotool, e.g. key('ctrl+shift+t')"""
        self.focus()
        xdo("key", "--clearmodifiers", "--delay", "40", *keys)

    def type(self, text: str) -> None:
        self.focus()
        xdo("type", "--delay", "8", "--", text)

    def run(self, command: str) -> None:
        """type a shell command and press enter"""
        self.type(command)
        self.key("Return")

    def mouse(self, x: int, y: int) -> None:
        """move the pointer to window coordinates"""
        xdo("mousemove", "--window", self.wid, str(x), str(y))

    def click(self, x: int, y: int, button: int = 1) -> None:
        self.focus()
        self.mouse(x, y)
        time.sleep(0.05)
        xdo("click", str(button))

    def drag(self, start: tuple, end: tuple) -> None:
        self.focus()
        self.mouse(*start)
        xdo("mousedown", "1")
        for i in range(1, 6):
            self.mouse(start[0] + (end[0] - start[0]) * i // 5, start[1] + (end[1] - start[1]) * i // 5)
            time.sleep(0.03)
        xdo("mouseup", "1")

    def wheel(self, x: int, y: int, up: bool, clicks: int = 5) -> None:
        self.focus()
        self.mouse(x, y)
        xdo("click", "--repeat", str(clicks), "--delay", "20", "4" if up else "5")

    def palette(self, query: str, run: bool = True) -> None:
        """open the command palette with its default key, type a query and run the best match"""
        self.key("ctrl+shift+p")
        time.sleep(0.4)
        self.type(query)
        time.sleep(0.2)
        if run:
            self.key("Return")

    def step(self, n: int) -> None:
        """let the script run past `step n`"""
        (self.tmp / f"step{n}").touch()

    def file(self, name: str) -> Path:
        """path inside the case temp dir, handy for shell side effects"""
        return self.tmp / name

    def wait_file(self, name: str, content: str | None = None, timeout: float = 10) -> str:
        path = self.file(name)

        def check():
            if not path.exists():
                return None
            text = path.read_text()
            if content is None or text.strip() == content:
                return text or " "
            return None

        return self.wait(check, timeout=timeout, msg=f"file {name} with {content!r}")

    def pty_size(self, name: str = "size") -> tuple:
        """(rows, cols) the shell sees, read with stty in an interactive bash"""
        path = self.file(name)
        path.unlink(missing_ok=True)
        self.run(f"stty size > {path}")
        return tuple(map(int, self.wait_file(name).split()))

    # layout, mirrors the element code so the tests can find cells and the bar

    def _get(self, *keys, default=None):
        value = self.settings if isinstance(self.settings, dict) else {}
        for key in keys:
            if not isinstance(value, dict) or key not in value:
                return default
            value = value[key]
        return value

    def font_size(self) -> float:
        default = bundled_settings()["terminal"]["font_size"]
        size = float(self._get("terminal", "font_size", default=default))
        return default if size < 6 else min(size, 72.0)

    def ui_font_size(self) -> float:
        default = bundled_settings()["ui_font_size"]
        size = float(self._get("ui_font_size", default=default))
        return default if size < 6 else min(size, 72.0)

    def bar_height(self) -> int:
        """tab bar height with its bottom border, when shown"""
        # 2 rems, the border is drawn inside that height
        return round(2 * self.ui_font_size())

    def cell_width(self) -> float:
        return self.font_size() * JB_ADVANCE

    def line_height(self) -> int:
        lh = self._get("terminal", "line_height", default=bundled_settings()["terminal"]["line_height"])
        value = {"standard": 1.3, "comfortable": 1.618}.get(lh) if isinstance(lh, str) else float(lh["custom"])
        if value < 1:
            value = 1.3
        value = min(value, 3.0)
        # rust rounds half away from zero
        return int(math.floor(self.font_size() * value + 0.5))

    def scrollbar_width(self) -> float:
        bar = bundled_settings()["terminal"]["scrollbar"]
        if self._get("terminal", "scrollbar", "enable", default=bar["enable"]) == "off":
            return 0.0
        width = float(self._get("terminal", "scrollbar", "width", default=bar["width"]))
        return float(bar["width"]) if width < 2 else min(width, 64.0)

    def expected_pty(self, w: int, h: int, top: int = 0) -> tuple:
        """(rows, cols) for a window of w x h with `top` pixels taken by the tab bar"""
        cw = self.cell_width()
        cols = int(max(w - cw - self.scrollbar_width(), cw * 2) // cw)
        rows = int(max(h - top, self.line_height()) // self.line_height())
        return rows, cols

    def cell_rect(self, col: int, row: int, cols: int = 1, rows: int = 1, top: int = 0) -> tuple:
        """(x0, y0, x1, y1) of a block of cells, top is the tab bar height when shown"""
        cw, lh = self.cell_width(), self.line_height()
        # cell (0, 0) sits one cell width in from the left edge, after a left scrollbar
        ox, oy = math.floor(cw), top
        if self._get("terminal", "scrollbar", "placement") == "left":
            ox = math.floor(cw + self.scrollbar_width())
        return (
            ox + math.floor(col * cw),
            oy + row * lh,
            ox + math.ceil((col + cols) * cw),
            oy + (row + rows) * lh,
        )

    def cells_color(self, col: int, row: int, cols: int, img=None, top: int = 0) -> tuple:
        """median color of the inside of a block of cells on one row, away from glyph edges"""
        img = self.shot() if img is None else img
        x0, y0, x1, y1 = self.cell_rect(col, row, cols, 1, top)
        lh = y1 - y0
        return median(img[y0 + lh // 3 : y1 - lh // 3, x0 + 3 : x1 - 3])
