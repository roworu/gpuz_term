"""virtual keyboard and pointer for the wayland e2e session, a plain wayland client with no dependencies.

headless sway has no input devices, so clients get no wl_keyboard or wl_pointer until some exist.
gpui only counts a window as active while a keyboard is there, and keyboards that come and go
make sway send modifiers before a keymap, so this keeps one of each alive for the whole session.
the keymap holds one keysym per keycode, added on first use, modifiers are set as a mask.
commands on stdin, one per line, each answered with "ok" once the compositor handled it:
  move X Y W H     absolute position X, Y in a layout of W x H logical pixels
  button N 1|0     press or release wl button code N (272 left, 273 right, 274 middle)
  wheel N          N wheel clicks, negative is up
  key MODS NAME    press and release keysym NAME with MODS held, like "ctrl,shift t" or "- Return"
  type HEX         type the utf-8 text given as hex, one keysym per character
  hold MODS        keep MODS down for the following keys and clicks, "-" lets go
"""

import os
import socket
import struct
import sys
import time

DISPLAY = 1
BTN_STATE = {"1": 1, "0": 0}
# real modifier bits, alt is Mod1 and logo Mod4 in xkbcommon
MODIFIERS = {"shift": 1, "ctrl": 4, "alt": 8, "super": 64}
# keysym names of printable ascii that are not the character itself
ASCII_NAMES = {
    " ": "space", "!": "exclam", '"': "quotedbl", "#": "numbersign", "$": "dollar", "%": "percent",
    "&": "ampersand", "'": "apostrophe", "(": "parenleft", ")": "parenright", "*": "asterisk",
    "+": "plus", ",": "comma", "-": "minus", ".": "period", "/": "slash", ":": "colon",
    ";": "semicolon", "<": "less", "=": "equal", ">": "greater", "?": "question", "@": "at",
    "[": "bracketleft", "\\": "backslash", "]": "bracketright", "^": "asciicircum",
    "_": "underscore", "`": "grave", "{": "braceleft", "|": "bar", "}": "braceright",
    "~": "asciitilde", "\n": "Return", "\t": "Tab",
}
# named keys tests press, in the first keymap so most runs never change it
NAMED = ["Return", "Escape", "Tab", "BackSpace", "Delete", "Left", "Right", "Up", "Down", "Home",
         "End", "Page_Up", "Page_Down", "Insert"] + [f"F{n}" for n in range(1, 13)]

# gpui guesses ascii from xkb keycodes aliasing us letters (24..61), which
# would turn synthesized function keys into characters, so start above that
KEYCODE_OFFSET = 62


def keysym_name(ch: str) -> str:
    if ch in ASCII_NAMES:
        return ASCII_NAMES[ch]
    if ch.isascii() and ch.isalnum():
        return ch
    return f"U{ord(ch):04X}"


class Wayland:
    def __init__(self) -> None:
        path = os.path.join(os.environ["XDG_RUNTIME_DIR"], os.environ.get("WAYLAND_DISPLAY", "wayland-0"))
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.next_id = 2
        self.buf = b""

    def new_id(self) -> int:
        self.next_id += 1
        return self.next_id - 1

    def send(self, obj: int, opcode: int, payload: bytes = b"", fd: int | None = None) -> None:
        data = struct.pack("<II", obj, (8 + len(payload)) << 16 | opcode) + payload
        if fd is None:
            self.sock.sendall(data)
        else:
            socket.send_fds(self.sock, [data], [fd])

    def events(self):
        """read whatever is buffered and yield (object, opcode, payload)"""
        self.buf += self.sock.recv(65536)
        while len(self.buf) >= 8:
            obj, word = struct.unpack("<II", self.buf[:8])
            size = word >> 16
            if len(self.buf) < size:
                break
            yield obj, word & 0xFFFF, self.buf[8:size]
            self.buf = self.buf[size:]

    def roundtrip(self, on_event=None) -> None:
        """wl_display.sync and wait for its done, failing on a protocol error"""
        callback = self.new_id()
        self.send(DISPLAY, 0, struct.pack("<I", callback))
        while True:
            for obj, opcode, payload in self.events():
                if obj == DISPLAY and opcode == 0:
                    code = struct.unpack("<II", payload[:8])[1]
                    raise RuntimeError(f"wayland error {code}: {string(payload[8:])[0]}")
                if obj == callback and opcode == 0:
                    return
                if on_event is not None:
                    on_event(obj, opcode, payload)

    def bind(self, registry: int, globals_: dict, interface: str, version: int) -> int:
        new = self.new_id()
        self.send(registry, 0, struct.pack("<I", globals_[interface]) + pack_string(interface)
                  + struct.pack("<II", version, new))
        return new


def string(data: bytes) -> tuple:
    """a wayland string from the start of data, and the bytes after it"""
    length = struct.unpack("<I", data[:4])[0]
    end = 4 + (length + 3) // 4 * 4
    return data[4 : 4 + length - 1].decode(), data[end:]


def pack_string(text: str) -> bytes:
    raw = text.encode() + b"\0"
    return struct.pack("<I", len(raw)) + raw + b"\0" * (-len(raw) % 4)


def fixed(value: float) -> int:
    return int(round(value * 256))


def now() -> int:
    return int(time.monotonic() * 1000) & 0xFFFFFFFF


class Keyboard:
    def __init__(self, wl: Wayland, keyboard: int) -> None:
        self.wl = wl
        self.keyboard = keyboard
        self.syms: list = []
        self.held = 0
        self.ensure(NAMED + [keysym_name(chr(c)) for c in range(32, 127)])

    def ensure(self, names: list) -> None:
        """give every keysym a keycode, uploading a new keymap when some were missing"""
        missing = [n for n in dict.fromkeys(names) if n not in self.syms]
        if not missing and self.syms:
            return
        self.syms += missing
        codes = "".join(f"<K{i}> = {i + KEYCODE_OFFSET};" for i in range(len(self.syms)))
        symbols = "".join(f"key <K{i}> {{[ {name} ]}};" for i, name in enumerate(self.syms))
        keymap = (f'xkb_keymap {{ xkb_keycodes "e2e" {{ minimum = 8; maximum = {len(self.syms) + KEYCODE_OFFSET - 1}; {codes} }};'
                  ' xkb_types "e2e" { include "complete" }; xkb_compatibility "e2e" { include "complete" };'
                  f' xkb_symbols "e2e" {{ {symbols} }}; }};\n').encode() + b"\0"
        fd = os.memfd_create("keymap")
        os.write(fd, keymap)
        # format 1 is xkb v1
        self.wl.send(self.keyboard, 0, struct.pack("<II", 1, len(keymap)), fd=fd)
        os.close(fd)
        self.wl.roundtrip()

    def modifiers(self, mask: int) -> None:
        self.wl.send(self.keyboard, 2, struct.pack("<IIII", mask, 0, 0, 0))

    def press(self, name: str, mods: int = 0) -> None:
        self.ensure([name])
        # keycodes on the wire are evdev ones, 8 below xkb's
        key = self.syms.index(name) + KEYCODE_OFFSET - 8
        self.modifiers(self.held | mods)
        self.wl.send(self.keyboard, 1, struct.pack("<III", now(), key, 1))
        self.wl.send(self.keyboard, 1, struct.pack("<III", now(), key, 0))
        self.modifiers(self.held)


def mask(mods: str) -> int:
    return 0 if mods == "-" else sum(MODIFIERS[m] for m in mods.split(","))


def main() -> None:
    wl = Wayland()
    registry = wl.new_id()
    wl.send(DISPLAY, 1, struct.pack("<I", registry))
    globals_ = {}

    def on_global(obj, opcode, payload):
        if obj == registry and opcode == 0:
            interface, _ = string(payload[4:])
            globals_[interface] = struct.unpack("<I", payload[:4])[0]

    wl.roundtrip(on_global)
    seat = wl.bind(registry, globals_, "wl_seat", 1)
    pointer_manager = wl.bind(registry, globals_, "zwlr_virtual_pointer_manager_v1", 1)
    keyboard_manager = wl.bind(registry, globals_, "zwp_virtual_keyboard_manager_v1", 1)
    pointer = wl.new_id()
    # no seat means the default one
    wl.send(pointer_manager, 0, struct.pack("<II", 0, pointer))
    keyboard_id = wl.new_id()
    wl.send(keyboard_manager, 0, struct.pack("<II", seat, keyboard_id))
    wl.roundtrip()
    keyboard = Keyboard(wl, keyboard_id)
    print("ready", flush=True)

    for line in sys.stdin:
        cmd, *args = line.split()
        if cmd == "move":
            x, y, w, h = map(float, args)
            # extents in hundredths keep fractional positions of scaled outputs
            wl.send(pointer, 1, struct.pack("<IIIII", now(), round(x * 100), round(y * 100),
                                            round(w * 100), round(h * 100)))
            wl.send(pointer, 4)
        elif cmd == "button":
            wl.send(pointer, 2, struct.pack("<III", now(), int(args[0]), BTN_STATE[args[1]]))
            wl.send(pointer, 4)
        elif cmd == "wheel":
            clicks = int(args[0])
            for _ in range(abs(clicks)):
                step = 1 if clicks > 0 else -1
                # source wheel, then one discrete step of the vertical axis
                wl.send(pointer, 5, struct.pack("<I", 0))
                wl.send(pointer, 7, struct.pack("<IIii", now(), 0, fixed(15 * step), step))
                wl.send(pointer, 4)
                wl.roundtrip()
                time.sleep(0.02)
        elif cmd == "key":
            keyboard.press(args[1], mask(args[0]))
        elif cmd == "type":
            text = bytes.fromhex(args[0]).decode() if args else ""
            keyboard.ensure([keysym_name(ch) for ch in text])
            for ch in text:
                keyboard.press(keysym_name(ch))
                wl.roundtrip()
                time.sleep(0.008)
        elif cmd == "hold":
            keyboard.held = mask(args[0])
            keyboard.modifiers(keyboard.held)
        else:
            raise SystemExit(f"unknown command {line!r}")
        wl.roundtrip()
        print("ok", flush=True)


if __name__ == "__main__":
    main()
