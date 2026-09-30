"""fake org.freedesktop.portal.Settings on the session bus, so gpui sees a system color scheme.

reads commands from stdin, one per line: a number sets color-scheme (0 no preference,
1 prefer dark, 2 prefer light) and emits SettingChanged like the real portal does.
prints "ready" once the bus name is owned.
"""

import sys

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

IFACE = "org.freedesktop.portal.Settings"
PATH = "/org/freedesktop/portal/desktop"
NS = "org.freedesktop.appearance"
KEY = "color-scheme"


class Portal(dbus.service.Object):
    def __init__(self, bus: dbus.Bus, scheme: int) -> None:
        super().__init__(bus, PATH)
        self.scheme = scheme

    def _check(self, ns: str, key: str) -> None:
        if ns != NS or key != KEY:
            raise dbus.exceptions.DBusException(
                "requested setting not found", name="org.freedesktop.portal.Error.NotFound"
            )

    # gpui's ashpd reads color-scheme with the deprecated Read and only unwraps one variant
    @dbus.service.method(IFACE, in_signature="ss", out_signature="v")
    def Read(self, ns: str, key: str):
        self._check(ns, key)
        return dbus.UInt32(self.scheme, variant_level=1)

    @dbus.service.method(IFACE, in_signature="ss", out_signature="v")
    def ReadOne(self, ns: str, key: str):
        self._check(ns, key)
        return dbus.UInt32(self.scheme, variant_level=1)

    @dbus.service.method(IFACE, in_signature="as", out_signature="a{sa{sv}}")
    def ReadAll(self, namespaces):
        return {NS: {KEY: dbus.UInt32(self.scheme, variant_level=1)}}

    @dbus.service.signal(IFACE, signature="ssv")
    def SettingChanged(self, ns, key, value):
        pass

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature="ss", out_signature="v")
    def Get(self, iface, prop):
        if prop == "version":
            return dbus.UInt32(2, variant_level=1)
        raise dbus.exceptions.DBusException("no such property", name="org.freedesktop.DBus.Error.InvalidArgs")

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature="s", out_signature="a{sv}")
    def GetAll(self, iface):
        return {"version": dbus.UInt32(2, variant_level=1)}

    def set_scheme(self, scheme: int) -> None:
        self.scheme = scheme
        self.SettingChanged(NS, KEY, dbus.UInt32(scheme, variant_level=1))


def main() -> None:
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    portal = Portal(bus, int(sys.argv[1]) if len(sys.argv) > 1 else 0)
    _name = dbus.service.BusName("org.freedesktop.portal.Desktop", bus, do_not_queue=True)
    loop = GLib.MainLoop()

    def on_stdin(source, condition) -> bool:
        line = sys.stdin.readline()
        if not line:
            loop.quit()
            return False
        line = line.strip()
        if line:
            portal.set_scheme(int(line))
            print("ok", flush=True)
        return True

    GLib.io_add_watch(sys.stdin, GLib.IO_IN | GLib.IO_HUP, on_stdin)
    print("ready", flush=True)
    loop.run()


if __name__ == "__main__":
    main()
