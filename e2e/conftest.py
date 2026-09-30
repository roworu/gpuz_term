"""e2e session: xvfb display, openbox for focus and size hints, a private session bus, the report"""

import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

sys.path.insert(0, str(Path(__file__).parent))

from harness import ARTIFACTS, BIN, App, Session, sh, wait_until, x_env
from report import Report

DISPLAY = os.environ.get("E2E_DISPLAY", ":99")

REPORT = Report()


@pytest.fixture(scope="session", autouse=True)
def x_session():
    assert BIN.exists(), f"binary {BIN} missing, run cargo build first"
    shutil.rmtree(ARTIFACTS, ignore_errors=True)
    Session.display = DISPLAY
    runtime = Path(Session.runtime_dir)
    shutil.rmtree(runtime, ignore_errors=True)
    runtime.mkdir(mode=0o700)
    procs = [subprocess.Popen(
        ["Xvfb", DISPLAY, "-screen", "0", "1600x1000x24", "-nolisten", "tcp", "-dpi", "96"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )]
    wait_until(lambda: subprocess.run(["xdpyinfo", "-display", DISPLAY], capture_output=True).returncode == 0,
               timeout=20, msg=f"xvfb on {DISPLAY}")
    bus = subprocess.run(
        ["dbus-daemon", "--session", "--fork", "--print-address=1", "--print-pid=1"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    Session.bus_address, bus_pid = bus[0], int(bus[1])
    procs.append(subprocess.Popen(["openbox"], env=x_env(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    # openbox is ready once it owns the wm selection
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        out = sh("xprop", "-root", "_NET_SUPPORTING_WM_CHECK", check=False, env=x_env())
        if "window id" in out:
            break
        time.sleep(0.1)
    yield
    for proc in procs:
        proc.terminate()
    for proc in procs:
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()
    subprocess.run(["kill", str(bus_pid)], capture_output=True)


@pytest.fixture
def app_factory(request):
    """start apps with App(...) arguments, all closed after the test, screenshots go to the report"""
    entry = REPORT.entry(request.node)
    apps = []
    # last screenshot per app, so an unchanged final state is not added twice
    last = {}

    def on_shot(app: App, caption: str) -> None:
        img = app.shot()
        if id(app) in last and np.array_equal(last[id(app)], img):
            return
        last[id(app)] = img
        path = ARTIFACTS / "shots" / f"{len(REPORT.entries):04d}-{len(entry.shots):02d}.png"
        path.parent.mkdir(parents=True, exist_ok=True)
        Image.fromarray(img).save(path, optimize=True)
        entry.shots.append((f"app {apps.index(app) + 1}: {caption}" if len(apps) > 1 else caption, path))

    def make(*args, start: bool = True, **kwargs) -> App:
        kwargs.setdefault("name", request.node.name[:40].replace("/", "_").replace("[", "_").replace("]", ""))
        app = App(*args, start=False, on_shot=on_shot, **kwargs)
        apps.append(app)
        entry.states.append(app)
        if start:
            app.start()
        return app

    yield make
    failed = getattr(request.node, "rep_call", None) is not None and request.node.rep_call.failed
    for app in apps:
        if app.alive():
            try:
                app.snap("at failure" if failed else "end of test")
            except Exception:
                pass
        if failed:
            entry.logs.append(app.output()[-3000:])
        app.close()
        entry.freeze(app)
        app.cleanup()


@pytest.fixture
def report_note(request):
    """add text to the report of this test, like output of a run without a window"""
    entry = REPORT.entry(request.node)
    return lambda title, text: entry.notes.append((title, text))


@pytest.hookimpl(hookwrapper=True)
def pytest_runtest_makereport(item, call):
    # lets fixtures know whether the test body failed, and the report what happened
    outcome = yield
    rep = outcome.get_result()
    setattr(item, "rep_" + rep.when, rep)
    REPORT.record(item, rep)


def pytest_sessionfinish(session, exitstatus):
    REPORT.write(ARTIFACTS / "report.html")
