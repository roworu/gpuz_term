//! mouse reports for programs that ask for the mouse, encoded as in xterm's ctlseqs "mouse tracking"

use alacritty_terminal::term::TermMode;
use gpui::Modifiers;

use super::Terminal;

// x10 puts each coordinate in one byte offset by 32, so 223 is the last one it can send
const X10_MAX_COORD: usize = 255 - 32;
// utf-8 mode puts the offset coordinate in one character of at most two bytes
const UTF8_MAX_COORD: usize = 0x7ff - 32;

/// button of a report, `None` is motion with no button held
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    None,
}

impl MouseButton {
    /// report button for a gpui button, none for buttons programs can't be told about
    pub fn from_gpui(button: gpui::MouseButton) -> Option<Self> {
        match button {
            gpui::MouseButton::Left => Some(Self::Left),
            gpui::MouseButton::Middle => Some(Self::Middle),
            gpui::MouseButton::Right => Some(Self::Right),
            gpui::MouseButton::Navigate(_) => None,
        }
    }

    fn code(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
            Self::None => 3,
            Self::WheelUp => 64,
            Self::WheelDown => 65,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

/// bytes telling the program about a mouse event at a 0 based visible `(column, line)`,
/// none when the modes in effect don't report it or the cell is past what the encoding can hold
pub fn mouse_report(
    mode: TermMode,
    (column, line): (usize, usize),
    button: MouseButton,
    action: MouseAction,
    modifiers: &Modifiers,
) -> Option<Vec<u8>> {
    if !mode.intersects(TermMode::MOUSE_MODE) {
        return None;
    }
    let reported = match action {
        // 1002 adds motion while a button is held, 1003 all motion
        MouseAction::Motion => {
            mode.contains(TermMode::MOUSE_MOTION)
                || (button != MouseButton::None && mode.contains(TermMode::MOUSE_DRAG))
        }
        // the wheel has no release, and a press needs a button
        MouseAction::Release => !matches!(
            button,
            MouseButton::WheelUp | MouseButton::WheelDown | MouseButton::None
        ),
        MouseAction::Press => button != MouseButton::None,
    };
    if !reported {
        return None;
    }
    let sgr = mode.contains(TermMode::SGR_MOUSE);
    let mut code = button.code();
    // only sgr tells which button went up, the older encodings send 3 for any release
    if action == MouseAction::Release && !sgr {
        code = 3;
    }
    if action == MouseAction::Motion {
        code += 32;
    }
    if modifiers.shift {
        code += 4;
    }
    if modifiers.alt || modifiers.platform {
        code += 8;
    }
    if modifiers.control {
        code += 16;
    }
    let (x, y) = (column + 1, line + 1);
    if sgr {
        let end = if action == MouseAction::Release {
            'm'
        } else {
            'M'
        };
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    let mut bytes = b"\x1b[M".to_vec();
    bytes.push(32 + code);
    if mode.contains(TermMode::UTF8_MOUSE) {
        if x > UTF8_MAX_COORD || y > UTF8_MAX_COORD {
            return None;
        }
        for value in [x, y] {
            let ch = char::from_u32(32 + value as u32)?;
            bytes.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
        }
    } else {
        if x > X10_MAX_COORD || y > X10_MAX_COORD {
            return None;
        }
        bytes.extend([32 + x as u8, 32 + y as u8]);
    }
    Some(bytes)
}

/// arrow keys for wheel `lines` on the alternate screen, positive is up, so programs like
/// less scroll with the wheel. none when the program did not keep alternate scroll on
pub fn alternate_scroll(mode: TermMode, lines: i32) -> Option<Vec<u8>> {
    if !mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) || lines == 0 {
        return None;
    }
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let key: &[u8] = match (lines > 0, app_cursor) {
        (true, false) => b"\x1b[A",
        (true, true) => b"\x1bOA",
        (false, false) => b"\x1b[B",
        (false, true) => b"\x1bOB",
    };
    Some(key.repeat(lines.unsigned_abs() as usize))
}

impl Terminal {
    /// true when the program asked for the mouse, shift keeps it for local selection
    pub fn owns_mouse(&self, modifiers: &Modifiers) -> bool {
        !modifiers.shift && self.last_content.mode.intersects(TermMode::MOUSE_MODE)
    }

    /// tell the program about a mouse event at a visible cell, if its modes report it
    pub fn report_mouse(
        &mut self,
        cell: (usize, usize),
        button: MouseButton,
        action: MouseAction,
        modifiers: &Modifiers,
    ) {
        if let Some(bytes) = mouse_report(self.last_content.mode, cell, button, action, modifiers) {
            self.write_to_pty(bytes);
        }
    }

    /// send the wheel to a program on the alternate screen as arrow keys, false when not wanted
    pub fn alternate_scroll(&mut self, lines: i32) -> bool {
        match alternate_scroll(self.last_content.mode, lines) {
            Some(keys) => {
                self.write_to_pty(keys);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLICK: TermMode = TermMode::MOUSE_REPORT_CLICK;
    const DRAG: TermMode = TermMode::MOUSE_DRAG;
    const MOTION: TermMode = TermMode::MOUSE_MOTION;
    const BUTTONS: [(MouseButton, u8); 3] = [
        (MouseButton::Left, 0),
        (MouseButton::Middle, 1),
        (MouseButton::Right, 2),
    ];

    fn none() -> Modifiers {
        Modifiers::default()
    }

    fn report(
        mode: TermMode,
        cell: (usize, usize),
        button: MouseButton,
        action: MouseAction,
        modifiers: &Modifiers,
    ) -> Option<String> {
        mouse_report(mode, cell, button, action, modifiers)
            .map(|bytes| String::from_utf8(bytes).unwrap())
    }

    fn sgr(mode: TermMode) -> TermMode {
        mode | TermMode::SGR_MOUSE
    }

    #[test]
    fn nothing_without_a_mouse_mode() {
        for extra in [TermMode::empty(), TermMode::SGR_MOUSE, TermMode::UTF8_MOUSE] {
            for action in [
                MouseAction::Press,
                MouseAction::Release,
                MouseAction::Motion,
            ] {
                let got = mouse_report(extra, (0, 0), MouseButton::Left, action, &none());
                assert_eq!(got, None, "{extra:?} {action:?}");
            }
        }
    }

    #[test]
    fn sgr_press_and_release_keep_the_button() {
        for (button, code) in BUTTONS {
            let press = report(sgr(CLICK), (4, 2), button, MouseAction::Press, &none());
            assert_eq!(press.as_deref(), Some(&*format!("\x1b[<{code};5;3M")));
            let release = report(sgr(CLICK), (4, 2), button, MouseAction::Release, &none());
            assert_eq!(release.as_deref(), Some(&*format!("\x1b[<{code};5;3m")));
        }
    }

    #[test]
    fn modifiers_add_to_the_button() {
        let cases = [
            (Modifiers::shift(), 4),
            (Modifiers::alt(), 8),
            (Modifiers::control(), 16),
            (Modifiers::control_shift(), 20),
            (
                Modifiers {
                    shift: true,
                    alt: true,
                    control: true,
                    ..Default::default()
                },
                28,
            ),
        ];
        for (modifiers, add) in cases {
            for (button, code) in BUTTONS {
                let got = report(sgr(CLICK), (0, 0), button, MouseAction::Press, &modifiers);
                assert_eq!(got, Some(format!("\x1b[<{};1;1M", code + add)));
            }
            let wheel = report(
                sgr(CLICK),
                (0, 0),
                MouseButton::WheelDown,
                MouseAction::Press,
                &modifiers,
            );
            assert_eq!(wheel, Some(format!("\x1b[<{};1;1M", 65 + add)));
        }
    }

    #[test]
    fn wheel_presses_only() {
        for mode in [CLICK, sgr(CLICK)] {
            for button in [MouseButton::WheelUp, MouseButton::WheelDown] {
                assert!(report(mode, (0, 0), button, MouseAction::Press, &none()).is_some());
                assert_eq!(
                    report(mode, (0, 0), button, MouseAction::Release, &none()),
                    None
                );
            }
        }
        let up = report(
            sgr(CLICK),
            (1, 1),
            MouseButton::WheelUp,
            MouseAction::Press,
            &none(),
        );
        assert_eq!(up.as_deref(), Some("\x1b[<64;2;2M"));
        let down = report(
            CLICK,
            (1, 1),
            MouseButton::WheelDown,
            MouseAction::Press,
            &none(),
        );
        assert_eq!(down.as_deref(), Some("\x1b[M\x61\x22\x22"));
    }

    #[test]
    fn motion_depends_on_the_mode() {
        let drag = |mode, button| report(sgr(mode), (2, 0), button, MouseAction::Motion, &none());
        // 1000 reports no motion at all
        assert_eq!(drag(CLICK, MouseButton::Left), None);
        assert_eq!(drag(CLICK, MouseButton::None), None);
        // 1002 reports motion with a button held
        assert_eq!(
            drag(DRAG, MouseButton::Left).as_deref(),
            Some("\x1b[<32;3;1M")
        );
        assert_eq!(
            drag(DRAG, MouseButton::Right).as_deref(),
            Some("\x1b[<34;3;1M")
        );
        assert_eq!(drag(DRAG, MouseButton::None), None);
        // 1003 reports all motion, no button is 3
        assert_eq!(
            drag(MOTION, MouseButton::None).as_deref(),
            Some("\x1b[<35;3;1M")
        );
        assert_eq!(
            drag(MOTION, MouseButton::Middle).as_deref(),
            Some("\x1b[<33;3;1M")
        );
        // 1002 and 1003 still report presses and releases
        for mode in [DRAG, MOTION] {
            let press = report(
                sgr(mode),
                (0, 0),
                MouseButton::Left,
                MouseAction::Press,
                &none(),
            );
            assert!(press.is_some());
        }
    }

    #[test]
    fn x10_encoding() {
        let bytes = |button, action| mouse_report(CLICK, (4, 2), button, action, &none()).unwrap();
        assert_eq!(
            bytes(MouseButton::Left, MouseAction::Press),
            b"\x1b[M\x20\x25\x23"
        );
        assert_eq!(
            bytes(MouseButton::Right, MouseAction::Press),
            b"\x1b[M\x22\x25\x23"
        );
        // any release is button 3
        for (button, _) in BUTTONS {
            assert_eq!(bytes(button, MouseAction::Release), b"\x1b[M\x23\x25\x23");
        }
        let motion = mouse_report(
            MOTION,
            (0, 0),
            MouseButton::None,
            MouseAction::Motion,
            &none(),
        );
        assert_eq!(motion.unwrap(), b"\x1b[M\x43\x21\x21");
    }

    #[test]
    fn x10_limit_is_223() {
        let at = |cell| mouse_report(CLICK, cell, MouseButton::Left, MouseAction::Press, &none());
        assert_eq!(at((222, 222)).unwrap(), b"\x1b[M\x20\xff\xff");
        assert_eq!(at((223, 0)), None);
        assert_eq!(at((0, 223)), None);
    }

    #[test]
    fn utf8_encoding_goes_past_223() {
        let mode = CLICK | TermMode::UTF8_MOUSE;
        let at = |cell| mouse_report(mode, cell, MouseButton::Left, MouseAction::Press, &none());
        // below 95 the coordinate is a single byte like in x10
        assert_eq!(at((4, 2)).unwrap(), b"\x1b[M\x20\x25\x23");
        // 32 + 96 = 128 needs two bytes
        assert_eq!(at((95, 0)).unwrap(), "\x1b[M\x20\u{80}\x21".as_bytes());
        assert_eq!(at((300, 0)).unwrap(), "\x1b[M\x20\u{14d}\x21".as_bytes());
        assert_eq!(
            at((2014, 2014)).unwrap(),
            "\x1b[M\x20\u{7ff}\u{7ff}".as_bytes()
        );
        assert_eq!(at((2015, 0)), None);
    }

    #[test]
    fn sgr_has_no_limit() {
        let got = report(
            sgr(CLICK),
            (5000, 3000),
            MouseButton::Left,
            MouseAction::Press,
            &none(),
        );
        assert_eq!(got.as_deref(), Some("\x1b[<0;5001;3001M"));
    }

    #[test]
    fn sgr_wins_over_utf8() {
        let mode = sgr(CLICK) | TermMode::UTF8_MOUSE;
        let got = report(mode, (0, 0), MouseButton::Left, MouseAction::Press, &none());
        assert_eq!(got.as_deref(), Some("\x1b[<0;1;1M"));
    }

    #[test]
    fn alternate_scroll_sends_arrows() {
        let alt = TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL;
        assert_eq!(alternate_scroll(alt, 3).unwrap(), b"\x1b[A\x1b[A\x1b[A");
        assert_eq!(alternate_scroll(alt, -2).unwrap(), b"\x1b[B\x1b[B");
        let app = alt | TermMode::APP_CURSOR;
        assert_eq!(alternate_scroll(app, 1).unwrap(), b"\x1bOA");
        assert_eq!(alternate_scroll(app, -1).unwrap(), b"\x1bOB");
        assert_eq!(alternate_scroll(alt, 0), None);
        // the normal screen scrolls history, and programs can turn it off
        assert_eq!(alternate_scroll(TermMode::ALTERNATE_SCROLL, 1), None);
        assert_eq!(alternate_scroll(TermMode::ALT_SCREEN, 1), None);
    }
}
