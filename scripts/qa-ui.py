#!/usr/bin/env python3
"""Minimal X11 input injector for EchoPort QA smoke tests.

Uses only the Python standard library plus the system libX11/libXtst that any
X11 desktop already has (loaded through ctypes — no pip packages, no build step).
It drives the real release binary to verify UI responsiveness during a live
network search, which cannot be done with the built-in demo/screenshot mode.

Examples (run under X11, e.g. `env -u WAYLAND_DISPLAY ./target/release/echoport &`):

    python3 scripts/qa-ui.py click 90 190
    python3 scripts/qa-ui.py type "Rick Astley"
    python3 scripts/qa-ui.py key Return
    python3 scripts/qa-ui.py key Escape
    python3 scripts/qa-ui.py motion 10 10

Coordinates are absolute X root pixels (XWayland physical pixels at scale 1.25).
The script is used by docs/QA_REPORT.md §search-responsiveness; it is not part of
the application and is safe to delete.
"""

from __future__ import annotations

import ctypes
import ctypes.util
import os
import sys
import time

X11 = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
XTST = ctypes.CDLL(ctypes.util.find_library("Xtst") or "libXtst.so.6")

X11.XOpenDisplay.restype = ctypes.c_void_p
X11.XOpenDisplay.argtypes = [ctypes.c_char_p]
X11.XFlush.argtypes = [ctypes.c_void_p]
X11.XWarpPointer.argtypes = [
    ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong,
    ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint,
    ctypes.c_int, ctypes.c_int,
]
X11.XStringToKeysym.restype = ctypes.c_ulong
X11.XStringToKeysym.argtypes = [ctypes.c_char_p]
X11.XKeysymToKeycode.restype = ctypes.c_ubyte
X11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
XTST.XTestFakeMotionEvent.restype = ctypes.c_int
XTST.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
XTST.XTestFakeButtonEvent.restype = ctypes.c_int
XTST.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
XTST.XTestFakeKeyEvent.restype = ctypes.c_int
XTST.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]

KEYSYMS = {
    " ": "space",
    ".": "period",
    ",": "comma",
    "-": "minus",
    "'": "apostrophe",
    "/": "slash",
    ":": "colon",
    ";": "semicolon",
    "?": "question",
    "!": "exclam",
}
NEEDS_SHIFT = {"?", "!", ":", "(", ")", "_", "+", '"', "@", "#", "$", "%", "&", "*"}

_display = None

# KWin can be configured with a global scale (e.g. 1.25). Synthetic XTest
# pointer coordinates from an XWayland client are then interpreted in logical
# pixels, while screenshots are in physical pixels. Set ECHOPORT_X11_SCALE to
# the same scale factor as the desktop to pass physical (screenshot) coords.
SCALE = float(os.environ.get("ECHOPORT_X11_SCALE", "1") or "1")


def display() -> ctypes.c_void_p:
    global _display
    if _display is None:
        name = os.environ.get("DISPLAY", ":0").encode()
        _display = X11.XOpenDisplay(name)
        if not _display:
            raise SystemExit(f"cannot open X display {name!r} (is DISPLAY set?)")
    return _display


def keycode(keysym_name: str) -> int:
    keysym = X11.XStringToKeysym(keysym_name.encode())
    if keysym == 0:
        raise SystemExit(f"unknown keysym: {keysym_name}")
    return X11.XKeysymToKeycode(display(), keysym)


def press(keysym_name: str, hold_ms: int = 18) -> None:
    code = keycode(keysym_name)
    XTST.XTestFakeKeyEvent(display(), code, 1, 0)
    X11.XFlush(display())
    time.sleep(hold_ms / 1000.0)
    XTST.XTestFakeKeyEvent(display(), code, 0, 0)
    X11.XFlush(display())


def type_text(text: str, per_key_ms: int = 22) -> None:
    shift = keycode("Shift_L")
    for char in text:
        name = KEYSYMS.get(char, char)
        with_shift = char.isupper() or char in NEEDS_SHIFT
        if with_shift:
            XTST.XTestFakeKeyEvent(display(), shift, 1, 0)
            X11.XFlush(display())
            time.sleep(0.008)
        press(name.lower() if char.isalpha() else name, hold_ms=12)
        if with_shift:
            XTST.XTestFakeKeyEvent(display(), shift, 0, 0)
            X11.XFlush(display())
        time.sleep(per_key_ms / 1000.0)


def click(x: int, y: int, button: int = 1, hold_ms: int = 30) -> None:
    x, y = int(round(x / SCALE)), int(round(y / SCALE))
    XTST.XTestFakeMotionEvent(display(), 0, x, y, 0)
    X11.XFlush(display())
    time.sleep(0.08)
    XTST.XTestFakeButtonEvent(display(), button, 1, 0)
    X11.XFlush(display())
    time.sleep(hold_ms / 1000.0)
    XTST.XTestFakeButtonEvent(display(), button, 0, 0)
    X11.XFlush(display())
    time.sleep(0.08)


def warp(x: int, y: int) -> None:
    """Move the pointer to absolute X root pixels (physical pixels).

    XWarpPointer is more reliable than XTestFakeMotionEvent on XWayland/KWin,
    where synthetic motion events can be ignored entirely.
    """
    root = X11.XDefaultRootWindow(display())
    X11.XWarpPointer(display(), 0, root, 0, 0, 0, 0, int(x), int(y))
    X11.XFlush(display())
    time.sleep(0.12)


def warp_click(x: int, y: int, button: int = 1, hold_ms: int = 30) -> None:
    """Warp to absolute root pixels, then click there via XTest."""
    warp(x, y)
    XTST.XTestFakeButtonEvent(display(), button, 1, 0)
    X11.XFlush(display())
    time.sleep(hold_ms / 1000.0)
    XTST.XTestFakeButtonEvent(display(), button, 0, 0)
    X11.XFlush(display())
    time.sleep(0.12)


def motion(x: int, y: int) -> None:
    x, y = int(round(x / SCALE)), int(round(y / SCALE))
    XTST.XTestFakeMotionEvent(display(), 0, x, y, 0)
    X11.XFlush(display())


def scroll(direction: str, count: int = 5, x: int | None = None, y: int | None = None) -> None:
    """Wheel scroll (button 4 = up, 5 = down), optionally after moving the pointer."""
    if x is not None and y is not None:
        # XWarpPointer is reliable on this KWin/XWayland setup (see warp()).
        warp(x, y)
    button = 4 if direction == "up" else 5
    for _ in range(max(1, count)):
        XTST.XTestFakeButtonEvent(display(), button, 1, 0)
        X11.XFlush(display())
        time.sleep(0.02)
        XTST.XTestFakeButtonEvent(display(), button, 0, 0)
        X11.XFlush(display())
        time.sleep(0.03)


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 2
    command = argv[1]
    rest = argv[2:]
    if command == "click" and len(rest) >= 2:
        click(int(rest[0]), int(rest[1]), int(rest[2]) if len(rest) > 2 else 1)
    elif command == "motion" and len(rest) >= 2:
        motion(int(rest[0]), int(rest[1]))
    elif command == "type" and rest:
        type_text(" ".join(rest))
    elif command == "key" and rest:
        press(rest[0])
    elif command == "scroll" and rest:
        direction = rest[0]
        count = int(rest[1]) if len(rest) > 1 else 5
        sx = int(rest[2]) if len(rest) > 3 else None
        sy = int(rest[3]) if len(rest) > 3 else None
        scroll(direction, count, sx, sy)
    elif command == "combo" and rest:
        # e.g. `combo Control_L v`
        modifiers = rest[:-1]
        for modifier in modifiers:
            XTST.XTestFakeKeyEvent(display(), keycode(modifier), 1, 0)
        X11.XFlush(display())
        press(rest[-1])
        for modifier in reversed(modifiers):
            XTST.XTestFakeKeyEvent(display(), keycode(modifier), 0, 0)
        X11.XFlush(display())
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
