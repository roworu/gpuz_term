"""safety: what programs printing escape sequences can and cannot do, hostile input and config, process cleanup"""

import base64
import os
import time

from harness import BASH, SCRIPT_PREAMBLE, App, bundled_theme, close_to, count, dominant, get_clipboard, set_clipboard

FEATURE = "safety"

DARK = bundled_theme(True)

# puts the tty in raw mode, sends a sequence, then keeps whatever the terminal answered
QUERY = (
    "step 1; stty raw -echo; printf '{sequence}'; "
    'timeout --foreground 2 cat > "$T2_TMP/got"; stty sane; touch "$T2_TMP/done"'
)


def query(app_factory, sequence: str) -> App:
    """app whose program sends a sequence once stepped, read the reply with replied()"""
    return app_factory({"theme": {"mode": "dark"}, "window_title": ["title"]},
                       script=QUERY.format(sequence=sequence))


def replied(app: App) -> str:
    app.step(1)
    app.wait_file("done", timeout=10)
    return app.file("got").read_text(errors="replace")


def abused(app_factory, script: str, **settings) -> App:
    """app whose first tab runs script, with a clean bash profile to check it afterwards"""
    app = app_factory({"theme": {"mode": "dark"}, **settings}, start=False,
                      commands={"commands": [{"name": "check in bash", "actions": [
                          {"new_tab_with_profile": "bash"}, {"type": 'touch "$T2_TMP/alive"\n'}]}]})
    path = app.file("abuse.sh")
    path.write_text(SCRIPT_PREAMBLE + script + "\nexec sleep 100000\n")
    app.settings["profiles"] = [
        {"name": "abuse", "command": {"with_arguments": {"program": "/bin/sh", "args": [str(path)]}}},
        {"name": "bash", "command": BASH},
    ]
    app.write("settings.jsonc", app.settings)
    app.start()
    return app


def responsive(app: App) -> None:
    """the app still opens tabs and runs commands after the abuse"""
    assert app.alive(), app.output()[-2000:]
    app.palette("check in bash")
    app.wait(app.file("alive").exists, msg="command in a new tab", timeout=15)


def gone(pid: int) -> bool:
    """ended, a zombie waiting for its new parent counts too"""
    try:
        return open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()[0] == "Z"
    except OSError:
        return True


def test_programs_cannot_read_the_clipboard(app_factory, report_note):
    """an osc 52 clipboard read gets no answer, so programs and remote hosts cannot steal it"""
    app = query(app_factory, "\\033]52;c;?\\007")
    set_clipboard("secret password")
    got = replied(app)
    report_note("reply to osc 52 read", repr(got))
    assert base64.b64encode(b"secret password").decode() not in got
    assert "52;" not in got


def test_programs_can_copy_to_the_clipboard(app_factory):
    """an osc 52 clipboard write is allowed, like ssh sessions copying text"""
    data = base64.b64encode(b"copied by a program").decode()
    app_factory({"theme": {"mode": "dark"}}, script=f"printf '\\033]52;c;{data}\\007'")
    time.sleep(0.5)
    deadline = time.monotonic() + 10
    while get_clipboard() != "copied by a program" and time.monotonic() < deadline:
        time.sleep(0.2)
    assert get_clipboard() == "copied by a program"


def test_title_is_not_reported_back(app_factory, report_note):
    """a title set by output is not typed back into the shell by a title report request"""
    app = query(app_factory, "\\033]2;rm -rf ~\\007\\033[21t")
    got = replied(app)
    report_note("reply to title report request", repr(got))
    assert "rm -rf" not in got


def test_paste_cannot_end_bracketed_paste_early(app_factory, report_note):
    """pasted text holding the end marker cannot break out and run as typed keys"""
    app = app_factory({"theme": {"mode": "dark"}},
                      script="printf '\\033[?2004h'; stty raw -echo; step 1; "
                             'timeout --foreground 3 cat > "$T2_TMP/got"; touch "$T2_TMP/done"')
    set_clipboard("safe\x1b[201~echo pwned\r")
    time.sleep(0.5)
    app.step(1)
    time.sleep(0.3)
    app.key("ctrl+shift+v")
    app.wait_file("done", timeout=10)
    got = app.file("got").read_text(errors="replace")
    report_note("bytes the program got", repr(got))
    assert got.count("\x1b[201~") == 1 and got.endswith("\x1b[201~"), repr(got)
    assert got.startswith("\x1b[200~")


def test_random_bytes_do_not_break_the_app(app_factory):
    """megabytes of random bytes, invalid utf-8 and broken sequences leave the app working"""
    app = abused(app_factory, 'head -c 3000000 /dev/urandom; printf "\\033c"; touch "$T2_TMP/done"')
    app.wait_file("done", timeout=60)
    app.snap("after random bytes")
    responsive(app)


def test_huge_line_and_title(app_factory):
    """a 1 MB line and a 1 MB title are handled without hanging"""
    app = abused(app_factory, 'head -c 1000000 /dev/zero | tr "\\0" x; echo; '
                             'printf "\\033]2;"; head -c 1000000 /dev/zero | tr "\\0" t; printf "\\007"; '
                             'touch "$T2_TMP/done"')
    app.wait_file("done", timeout=60)
    responsive(app)


def test_escape_sequence_storm(app_factory):
    """thousands of mode switches, resizes requests and colors do not crash"""
    storm = ("i=0; while [ $i -lt 3000 ]; do "
             "printf '\\033[?1049h\\033[?1000h\\033[8;5;5t\\033]11;#ff0000\\007\\033[38;5;%dm\\033[H\\033[2J"
             "\\033[?1049l\\033[?25l\\033[5 q\\033[?2004h' $((i % 256)); i=$((i+1)); done; "
             "printf '\\033c'; touch \"$T2_TMP/done\"")
    app = abused(app_factory, storm)
    app.wait_file("done", timeout=90)
    assert app.size() == (900, 600), "programs cannot resize the window"
    responsive(app)


def test_extreme_config_values_are_limited(app_factory):
    """absurd numbers in settings are limited, the app starts and paints"""
    app = abused(app_factory, "", ui_font_size=1e30, tab_width=4294967295,
                 terminal={"font_size": -1e30, "line_height": {"custom": 1e9},
                           "max_history_length": 18446744073709551615,
                           "scrollbar": {"width": 1e9, "auto_hide": -5}, "smooth_scroll": {"duration": 1e12}},
                 notifications={"timeout": 1e20})
    app.wait(lambda: close_to(dominant(app.shot()), DARK["terminal_background"]), msg="painted")
    responsive(app)


def test_hanging_exec_title_does_not_freeze(app_factory):
    """a title exec block that never ends does not block typing or tabs"""
    app = abused(app_factory, "", window_title=[{"exec": "sleep 1000"}, "number"])
    time.sleep(2)
    start = time.monotonic()
    responsive(app)
    assert time.monotonic() - start < 10


def test_closing_a_tab_ends_its_programs(app_factory):
    """closing a tab ends its shell and the program running in it"""
    app = app_factory({"theme": {"mode": "dark"}, "window_title": ["number"]})
    app.key("ctrl+shift+t")
    app.wait_title("2")
    app.run(f"echo $$ > {app.file('shell')}; sleep 1000 & echo $! > {app.file('job')}; sleep 1000")
    shell = int(app.wait_file("shell"))
    job = int(app.wait_file("job"))
    assert os.path.exists(f"/proc/{shell}") and os.path.exists(f"/proc/{job}")
    app.key("ctrl+shift+w")
    # sleep runs in the foreground, so the tab asks before closing
    app.wait(lambda: count(app.shot(), DARK["danger_button"]) > 200, msg="close tab dialog")
    app.snap("asked before closing the busy tab")
    app.key("Return")
    app.wait_title("1")
    app.wait(lambda: gone(shell), msg="shell ended")
    app.wait(lambda: gone(job), msg="background job ended")


def test_quitting_ends_every_shell(app_factory):
    """quitting leaves no shells of any tab behind"""
    app = app_factory({"theme": {"mode": "dark"}, "window_title": ["number"]})
    shells = []
    for n in range(3):
        if n:
            app.key("ctrl+shift+t")
            app.wait_title(str(n + 1))
        app.run(f"echo $$ > {app.file(f'shell{n}')}")
        shells.append(int(app.wait_file(f"shell{n}")))
    for _ in range(3):
        app.key("ctrl+shift+w")
    app.wait(lambda: not app.alive(), msg="app exit")
    app.wait(lambda: all(gone(pid) for pid in shells), msg="all shells ended")
