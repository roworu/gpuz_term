# e2e extension plan: fix the blind spots, test all of them

Goal: close every blind spot found in the e2e review on 2026-09-30. That means fixing the app gaps (quit warning,
dialog wording, dialog keys, mouse reporting) and adding e2e coverage for everything, including a second
e2e backend on Wayland.

## 0. Starting state (read first)

- Branch `release/v0.0.5`. This session left **uncommitted** e2e changes; review and commit them before starting:
  - `e2e/test_safety.py`: `test_closing_a_tab_ends_its_programs` confirms the new close dialog.
  - new `e2e/test_close_dialog.py` (11 cases); `show_tab_close_button`, `pick_tab` and arrow-wrap tests.
  - `harness.unique_theme` includes `danger_button` and `danger_button_text`.
  - `conftest.py` adds the window title to every screenshot caption.
  - Stronger checks: `test_fixed_mode_ignores_desktop` now sleeps before asserting, the alpha hex colors are
    checked, and the profile theme test switches back.
  - Full suite: 251 passed.
- Rules from AGENTS.md apply:
  - Build and test in podman (`sh .zed/podman-build-test.sh "$PWD"`); run e2e with `sh e2e/run.sh`.
  - New functionality needs unit tests (`mod tests` at the bottom of the file) plus e2e tests.
  - New settings go in `assets/default_settings.jsonc` with a comment; never hardcode defaults in rust.
  - Never assert bundled default values in tests.
- `e2e/run.sh` wipes `e2e/artifacts` on every run, so a `-k` run replaces the full report. Do a full run last.
- License: zed's `terminal` crate (with `mappings/mouse.rs`) is **GPL-3.0**, and kuterm is MIT/Apache.
  Do not copy from it. Write mouse encoding from the xterm spec (ctlseqs, "Mouse Tracking").

Work order: A (app fixes, X11 e2e) → B (new X11 e2e areas) → C (mouse reporting) → D (Wayland backend).
Each step ends green on `.zed/podman-build-test.sh` and `e2e/run.sh`.

---

## A. Close and quit warnings

### A1. Warn on quit and on window close

The gap: the `quit` action (`CommandAction::Quit => cx.quit()`, `src/ui/workspace/mod.rs:525`) and the window
manager's close (`main.rs:70` `on_window_closed → quit`) end every running program without asking.

- `Workspace::running_programs(&self, cx) -> Vec<(usize, String)>`: tab index and program name for every tab
  where `running_program(ix)` is some.
- Generalize `ConfirmClose` (`src/ui/command_palette.rs`):
  - Replace `tab: EntityId` with a target enum `CloseTarget::Tab(EntityId) | CloseTarget::Window`.
  - The message is built by the caller (see A2).
  - Keep `CloseConfirmed`. The workspace matches on the target: close that tab, or `cx.quit()`.
- The quit action goes through `request_quit(window, cx)`:
  - If `close_running_tab_warn` is on and some tab is busy, open the dialog.
  - Otherwise quit.
- Window close: in `Workspace::new` (it has the window), register
  `window.on_window_should_close(cx, ...)`. The API is in gpui `window.rs:6688`, and X11 and Wayland both
  implement `on_should_close`. The callback returns `true` when nothing is running or the warning is off.
  Otherwise it opens the dialog and returns `false`. A confirmed dialog then calls `cx.quit()`; the existing
  `on_window_closed` handler stays for the normal path.
- Update the comment of `close_running_tab_warn` in `assets/default_settings.jsonc`: it also covers quitting and
  closing the window. Keep the name; renaming breaks user configs.
- Unit tests (workspace `mod tests`):
  - A busy tab plus the `quit` action shows the dialog; escape keeps everything.
  - Enter quits. Needs a check that the test app is quitting; see how existing tests check `cx.quit`.
  - Idle tabs quit at once, and so does a disabled warning.
  - The should-close callback returns false with a busy tab and true when idle. Call the registered handler
    through the gpui test window if it's reachable; otherwise pull the logic into a fn and test that.

### A2. The dialog names the tab

"is still running in this tab" is wrong when the tab was middle-clicked or its × was clicked while it isn't
the active tab.

- Tab: `"vim" is still running in tab 3 "<tab title>"`, using `tab_title(ix)`.
- Window: `2 tabs are still running programs: vim, cargo`, listing up to 3 names and then "…".
- Unit test: middle-click close of an inactive busy tab closes **that** tab, not the active one, and the
  message contains its number.

### A3. Stricter dialog keys

`ConfirmClose::key_down` matches `keystroke.key` only, so ctrl+y confirms and ctrl+n cancels. This comes from
reading the code; it's untested.

- `y`, `n`, `enter` and `escape` count only with no modifiers; shift is allowed for `y`/`n`. Arrows ignore
  modifiers.
- Decide what happens to app keybindings while the dialog is focused (`ctrl-shift-t`, `ctrl-shift-w`,
  `ctrl-shift-p`, `alt-N`). The dialog's `on_key_down` calls `stop_propagation`, but gpui may dispatch bound
  actions before or after it. Find out, then pin the behavior with tests. Recommended:
  - ctrl-shift-w while the dialog is open does nothing; it must not stack a second dialog.
  - ctrl-shift-p replaces the dialog with the palette. That's the current `open_overlay` behavior; keep it
    and test it.
  - ctrl-shift-t opens a tab and the dialog closes, because focus moves to the new tab.
- Unit tests for each key, plus e2e (below).

### A e2e (X11 now, Wayland later through the same harness)

Add these to `e2e/test_close_dialog.py`:
- `test_quit_action_asks_with_running_program`:
  - The palette `quit` with a busy tab shows the dialog, which names the program; take a screenshot.
  - Escape keeps the app alive.
  - Quitting again, then Enter, ends the app, and the program's pid is gone (reuse `gone()` from `test_safety`).
- `test_window_close_asks_with_running_program`: close through the window manager.
  - Add `wmctrl` to `e2e/Containerfile`. `wmctrl -i -c <wid>` sends `_NET_CLOSE_WINDOW`, and openbox turns
    that into `WM_DELETE_WINDOW`.
  - The dialog shows; escape leaves the window mapped; closing again + Enter makes the app exit with code 0.
  - Harness: `App.wm_close()`.
- `test_window_close_idle_quits_at_once`: no dialog, the app exits and the shells end.
- `test_disabled_warning_quits_at_once`: `close_running_tab_warn: false` with the quit action and window close.
- `test_dialog_names_inactive_tab`:
  - Three tabs; tab 2 is busy; tab 3 is active; middle-click tab 2.
  - The dialog shows. Check it's there by pixels; also check the text by width: the message row is wider than
    in the one-tab case, or use a distinctive tab title and compare `ink` widths. A screenshot goes in the report.
  - Enter leaves the title at "2" (tab 3 became tab 2 and is still active), and tab 2's program pid is gone.
- `test_modified_keys_do_not_answer`: ctrl+y and ctrl+n leave the dialog open.
- `test_new_dialog_does_not_stack`: ctrl-shift-w twice, escape once, and the dialog is gone.
- `test_palette_replaces_dialog`: ctrl-shift-p while asked shows the palette, and the tab is kept.
- `test_dialog_light_theme`:
  - `theme.mode: light` and the bundled light theme; the dialog shows in its `danger_button` color.
  - Take a screenshot.
  - Don't assert bundled values as literals; read them with `bundled_theme(False)`.

---

## B. New X11 e2e areas (tests only, unless a test finds a bug)

### B1. Wide and combining characters — new `e2e/test_unicode.py`

- The container needs fonts with CJK and emoji glyphs so screenshots aren't tofu. Add `fonts-noto-cjk` (large;
  `fonts-wqy-microhei` is lighter) and `fonts-noto-color-emoji` to `e2e/Containerfile`. Positions are checked
  with markers either way, so the tests don't depend on these glyphs.
- Cases:
  - `printf '日本X'`: `X` ink sits in `cell_rect(4, 0)`, and the cursor block is at col 5 (reuse `cursor`
    helpers with the block shape).
  - Emoji `printf '😀X'`: X at col 2.
  - Combining `printf 'e\xcc\x81X'`: X at col 1; the `é` ink stays inside cell 0, and the accent is drawn above
    the e (ink rows above the x-height).
  - A wide char at the last column wraps to the next line: fill cols-1 cells, then a wide char. It goes to
    row 1, col 0.
  - Selecting and copying a wide char with a double click gives back the exact UTF-8.
  - Titles: an OSC 2 title with CJK and emoji is set as the window title exactly (`app.title()`).
  - Tab titles with wide chars get cut with an ellipsis, not garbage: a screenshot plus ink inside the tab.

### B2. Triple click — `e2e/test_keys.py`

- `echo alpha bravo charlie`; triple click (`xdotool click --repeat 3 --delay 80 1`) on bravo; ctrl-shift-c.
  The clipboard holds the whole line, `alpha bravo charlie`.
- Shift+click extends a selection (`mouse_down` supports it): click on alpha, shift+click on charlie, copy,
  and the clipboard holds the full line.

### B3. Selection follows scrollback — `e2e/test_keys.py` or `test_scroll.py`

- Print `MARK`; select it by dragging; then the script prints 5 more lines (use `step`).
- The selection color moves up 5 rows in pixels, and copy still gives `MARK`.
- Scroll with the wheel while selected: the selection moves with the view.
- Output large enough to push `MARK` out of history (`max_history_length` small): the selection clears
  without crashing, and copy gives an empty clipboard or no change. Pin whatever the app does, as long as it
  doesn't crash.

### B4. HiDPI scale — new `e2e/test_scale.py`

- `x_env()` hardcodes `GPUI_X11_SCALE_FACTOR=1`. Let `App(env={"GPUI_X11_SCALE_FACTOR": "2"})` override it
  (it already can, through `env_extra`), and add `App.scale`, read from that env.
- Xvfb is 1600x1000, but 900x600 logical at scale 2 is 1800x1200. Raise the screen in `conftest.py` to
  `3840x2160x24`. Check that nothing assumes 1600x1000 (the focus-thief geometry `+1200+800` in
  `test_cursor.py` is fine).
- The layout helpers in `harness.py` (`cell_width`, `line_height`, `bar_height`, `cell_rect`,
  `scrollbar_width`) return physical pixels: multiply by `self.scale`. Check how gpui rounds at 1.5; line
  heights are rounded in logical px, then scaled.
- Cases, for scale 2 and 1.5:
  - The window is 900x600 × scale physical.
  - The pty size is the same as at scale 1.
  - The red block from `test_font.SAMPLE` covers exactly `cell_rect(0, 0, 10, 2)` at scale.
  - The tab bar height scales.
  - The bar cursor is `2*scale` wide. Check `cursor.rs` for how the bar width is defined; the test must follow
    the code's intent.
  - A drag selection hits the right cells: copy gives the expected text.
  - Scrollbar width scales.
  - Screenshot each case.
- Risk: with lavapipe, scale 1.5 may produce blurry text; assert positions only.

### B5. Non-US keyboard layouts — new `e2e/test_layouts.py`

- Container: `x11-xkb-utils` (setxkbmap) and `xkb-data`.
- Fixture `layout(name)`: runs `setxkbmap -display :99 <name>` and resets to `us` on teardown, because the X
  session is shared across tests.
- xdotool maps keysyms to the current layout by itself when typing, so `app.type()` keeps working.
- Cases:
  - `de`: type `zyäöüß@€` (xdotool types `@` as AltGr+q). The shell receives exactly that; check with
    `echo ... > file`.
  - `de`: the default keybinding `ctrl-shift-t` still opens a tab. xdotool sends the keysym `t`, which is the
    same key on de. Also bind `ctrl-shift-z` and press the physical key that is `z` on de
    (`xdotool key --clearmodifiers ctrl+shift+z` resolves the keysym). The action runs.
  - `ru` (non-Latin): `ctrl-shift-t` still opens a tab, because gpui matches the key by its Latin keysym. If
    that fails, it's a real finding; report it rather than weakening the test.
  - Dead keys on `us(intl)`: `xdotool key dead_acute e` produces `é` in the shell (gpui X11 compose).
  - A US keybinding typed on the `fr` (azerty) layout: `alt-1` on azerty is `alt+ampersand` physically. Decide
    what is expected (keysym or physical key), then pin it.

### B6. Smaller gaps from the screenshot review

- `test_palette.panel()` matches the tab bar when there are 2+ tabs. Take a `top` offset
  (`panel(app, img[h:])`, as the pick_tab test already does) or exclude rows `< bar_height`.
- Tests whose screenshots show nothing useful get a `snap` at the state under test:
  - `test_paste_is_bracketed_only_when_asked`
  - `test_profile_command_with_arguments`
  - `test_protocol.*` already has notes; fine.

---

## C. Mouse reporting to programs (new app feature)

Right now all clicks go to selection and the wheel always scrolls history (`terminal_view.rs:127`,
`terminal_element/mod.rs:72-110`, left button only). vim, htop, tmux and less --mouse get no mouse input.

### C1. Encoding: new `src/terminal/mouse.rs` (pure functions, written from the xterm spec)

- Inputs:
  - `TermMode`.
  - Grid point: 0-based col and line, taken from `selection.rs::mouse_point`. Make that callable here.
  - Button: Left, Middle, Right, WheelUp, WheelDown, None for motion.
  - Action: Press, Release, Motion.
  - Modifiers: shift=4, alt/meta=8, ctrl=16.
- Output: `Option<Vec<u8>>`.
- Modes, all alacritty `TermMode` flags:
  - `MOUSE_REPORT_CLICK` (1000): presses and releases.
  - `MOUSE_DRAG` (1002): adds motion while a button is held (+32).
  - `MOUSE_MOTION` (1003): adds all motion; no button is 3+32=35.
  - `SGR_MOUSE` (1006): `ESC [ < b ; x ; y M|m`, 1-based, release keeps its button with `m`.
  - `UTF8_MOUSE` (1005): coordinates as UTF-8 above 95.
  - Default X10 encoding: `ESC [ M` then `32+b`, `32+x`, `32+y` bytes. Release is b=3. Coordinates past 223
    get no report.
  - Wheel is 64/65, press only.
- Unit tests in the same file: every mode × button × modifier, the coordinate limits, releases.

### C2. Wiring

- `TerminalView`:
  - If any mouse mode is on and shift isn't held, `mouse_down`, `mouse_up` and drag send reports through
    `terminal.input(bytes)` instead of selecting. Shift forces local selection, which is the xterm and
    alacritty convention.
  - Add middle and right buttons and motion without a pressed button in `terminal_element/mod.rs` (only while
    `MOUSE_MOTION` is set, to keep idle cost at zero).
  - Only send motion when the cell changes.
- Wheel:
  - With a mouse mode on, send wheel reports, one per line of delta.
  - Else, if `ALT_SCREEN` and `ALTERNATE_SCROLL` (1007) are on, send up/down arrow keys, `ESC O A` under
    `APP_CURSOR`.
  - Otherwise scroll history as now.
  - Smooth scroll doesn't apply to reports.
- Middle-click paste: out of scope unless primary selection exists. Right now middle click in the terminal
  does nothing; keep that.
- The scrollbar keeps working: clicks on it are handled before mouse reporting.
- Setting: none needed. If a switch is wanted later, add `terminal.mouse_reporting` in `default_settings.jsonc`
  with a comment.

### C3. Tests

- Unit tests: C1 plus the view routing (shift forces selection; mode off selects).
- e2e, new `e2e/test_mouse.py`:
  - Reuse the `QUERY`-style script from `test_safety.py`: `printf` the mode sequence, `stty raw -echo`,
    `timeout --foreground 2 cat > got`.
  - Click at `cell_rect(4, 2)` and expect `\e[<0;5;3M` then `\e[<0;5;3m`.
  - Right and middle buttons.
  - Wheel up/down gives 64 and 65.
  - 1002 drag gives motion reports with +32.
  - 1003 motion without buttons gives 35.
  - ctrl+click adds 16.
  - X10 encoding without 1006.
  - 1005 UTF-8 encoding: needs a window wide enough for col > 95 at a small font size.
  - Shift+click with the mode on: no bytes, and the selection is painted.
  - Mode off: a click gives no bytes (current behavior).
  - Alternate scroll: `\e[?1049h\e[?1007h`, then the wheel gives `\e[A` × N, and `\eOA` with `\e[?1h`.
  - Optionally a real program: add `vim-tiny` or `less` to the container;
    `less --mouse` over a 300-line file scrolls on wheel. Check the ink moved.

---

## D. Wayland e2e backend

The same tests run against a headless wlroots compositor, so both gpui backends (x11 and wayland, both
enabled in `Cargo.toml`) are covered.

### D0. Spike first (a half day, go/no-go)

- Stack: `sway` (headless) + `grim` (screenshots through wlr-screencopy) + `wtype` (keys through
  virtual-keyboard) + `swaymsg seat` (pointer) + `wl-clipboard`.
- Check that the Ubuntu 26.04 packages exist: `sway grim wtype wl-clipboard`.
- Start it: `WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 sway -c e2e/sway.conf`, with
  `XDG_RUNTIME_DIR` set and no `DISPLAY`.
- Run kuterm with `WAYLAND_DISPLAY=wayland-1` and `DISPLAY` unset. Check that gpui picks Wayland and that
  wgpu with lavapipe presents on wayland WSI (shm fallback).
- The biggest risk: if lavapipe can't present on a pixman-rendered headless output, try `WLR_RENDERER=gles2`
  with llvmpipe (mesa EGL surfaceless). If neither works, stop and report back.
- Check decorations: gpui on Wayland asks for server-side decorations through xdg-decoration. Sway grants
  them; with `default_border none` the client rect is the window rect. If gpui draws CSD instead, measure the
  offset and account for it in `geometry()`.

### D1. `e2e/sway.conf`

```
output HEADLESS-1 resolution 1600x1000 scale 1
default_border none
default_floating_border none
focus_follows_mouse no
for_window [app_id=".*"] floating enable
seat seat0 hide_cursor 0
```

- For HiDPI, run a variant with `scale 2` or `1.5`, switched at runtime with `swaymsg output HEADLESS-1 scale 2`.

### D2. Harness split

- `harness.py` gets a `Backend` interface with `X11Backend` (the current code moved in) and `WaylandBackend`.
  `App` calls only the backend.

| operation | X11 (now) | Wayland |
|---|---|---|
| start session | Xvfb + openbox (conftest) | headless sway; wait for its socket and `swaymsg -t get_version` |
| app env | `DISPLAY`, `GPUI_X11_SCALE_FACTOR` | `WAYLAND_DISPLAY`, no `DISPLAY` |
| find window | `xdotool search --pid` | `swaymsg -t get_tree`, find the node with `pid` |
| geometry | `xwininfo` | node `rect` (+ CSD offset from D0) |
| screenshot | `ImageGrab` | `grim -g "x,y wxh" -` → PIL |
| key / type | `xdotool key/type` | `wtype -M ctrl -M shift -k t -m shift -m ctrl`, `wtype text` (map names: Return→Return, ctrl+shift+t → modifier flags) |
| mouse move / click / wheel | `xdotool` | `swaymsg seat seat0 cursor set X Y`, `cursor press/release button1..3`, wheel `button4/5` |
| drag | xdotool mousedown/move/up | cursor press, several `cursor set`, release |
| focus | `windowactivate` | `swaymsg '[pid=N] focus'` |
| resize | `xdotool windowsize` | `swaymsg '[pid=N] resize set W H'` |
| title | `xdotool getwindowname` | node `name` |
| app id / class | `xprop WM_CLASS` | node `app_id` |
| min size hint | `xprop WM_NORMAL_HINTS` | no hint query; test it with a resize to 1x1 |
| clipboard | `xclip` | `wl-copy` / `wl-paste` |
| window close | `wmctrl -c` | `swaymsg '[pid=N] kill'` (xdg_toplevel.close → `on_should_close`) |
| focus thief | `xmessage` | a second kuterm with its own config, or `swaymsg create_output` + focus it |
| keyboard layout | `setxkbmap` | `swaymsg input type:keyboard xkb_layout de` |

- Coordinates: xdotool moves relative to the window, sway `cursor set` is in output coordinates. The backend
  adds the window rect.
- `conftest.py`:
  - Add a `--backend` pytest option (default `x11`), and also read `E2E_BACKEND`.
  - Start the matching session.
  - Report: put the backend in the title and in each entry, e.g. "kuterm e2e report (wayland)".
- Markers: `@pytest.mark.x11_only` for xprop-specific tests (`test_min_size_hint_is_set`, `WM_CLASS`) and
  `@pytest.mark.wayland_only` for the app_id check. Skip tests marked for the other backend, with a reason.
- `e2e/run.sh`: pass `E2E_BACKEND` through `-e`; `sh e2e/run.sh --backend wayland -k palette` should work.
- Report files: `e2e/artifacts/report-x11.html` and `report-wayland.html`, so both survive. Update
  `release.yml`, which uploads `report.html`.

### D3. CI

- `.github/workflows/release.yml` e2e job: a matrix of `backend: [x11, wayland]`, one artifact per backend
  (`e2e-report-${{ matrix.backend }}`); `publish` needs both.
- Update AGENTS.md "Build and test" with the backend flag.

### D4. Wayland-specific tests

- All existing tests pass under `--backend wayland`, except the marked ones.
- Wayland only:
  - `app_id` is `kuterm`.
  - `swaymsg kill` goes through should-close (A1).
  - Clipboard round trip through `wl-copy`/`wl-paste`.
  - Fractional scale 1.5 through `swaymsg output ... scale 1.5`: cell geometry and pty size as in B4.
  - Keyboard layout switch through `swaymsg input`.
  - Focus loss when another window is focused makes the cursor hollow.

---

## Done when

- `sh .zed/podman-build-test.sh "$PWD"` is green: build, clippy `-D warnings`, fmt, unit tests.
- `sh e2e/run.sh` (x11) and `sh e2e/run.sh --backend wayland` are both fully green.
- Screenshots in both reports are reviewed one by one, and every caption matches what's shown.
- AGENTS.md is updated (backend flag, new container packages). `default_settings.jsonc` comments are updated.
- The user reviews the changes before any commit or PR.
