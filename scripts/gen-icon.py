#!/usr/bin/env python3
"""Generate the EchoPort application icon with the Python standard library only.

Brand: a porcelain-white rounded square with a single macaron/blue accent dot
(the same motif the app paints in its window icon). No external image tools,
no Pillow, no network: just a tiny PNG writer on top of `zlib` + `struct`.

Usage:
    python3 scripts/gen-icon.py [--out-dir assets] [--size 256] [--size 128]

Outputs:
    assets/icon.png       256x256 (default; the file referenced by the .desktop)
    assets/icon-128.png   128x128 (optional extra size)

The generator is deterministic, so re-running it over an existing icon is safe.
"""

from __future__ import annotations

import argparse
import math
import struct
import zlib
from pathlib import Path

# Brand colours (keep in sync with src/theme.rs / src/main.rs).
PORCELAIN_TOP = (251, 253, 255)   # near-white, top of the tile
PORCELAIN_BOTTOM = (238, 244, 252)  # cool porcelain, bottom of the tile
BORDER = (220, 231, 245)          # soft porcelain-blue border
ACCENT = (59, 130, 198)           # #3B82C6 porcelain blue
ACCENT_LIGHT = (111, 169, 221)    # lighter blue for the gradient rim
MACARON_PINK = (247, 200, 216)    # strawberry macaron
MACARON_LILAC = (217, 204, 242)   # lavender macaron


def _clamp(value: float, low: float = 0.0, high: float = 1.0) -> float:
    return low if value < low else high if value > high else value


def _mix(a: tuple[int, int, int], b: tuple[int, int, int], t: float) -> tuple[int, int, int]:
    t = _clamp(t)
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def _sd_round_rect(x: float, y: float, half: float, radius: float) -> float:
    """Signed distance to a centred rounded square (negative inside)."""
    qx = abs(x) - (half - radius)
    qy = abs(y) - (half - radius)
    ax = max(qx, 0.0)
    ay = max(qy, 0.0)
    return math.sqrt(ax * ax + ay * ay) + min(max(qx, qy), 0.0) - radius


def _coverage(distance: float) -> float:
    """Anti-aliased coverage for a shape edge at `distance` (1 inside, 0 outside)."""
    return _clamp(0.5 - distance)


def render(size: int) -> bytes:
    """Render one icon as RGBA rows (top-down), size x size."""
    half = size / 2.0
    # Rounded-square geometry: a generous squircle-like radius.
    radius = size * 0.225
    # Accent dot: soft radial blue -> pink, plus a thin macaron ring.
    dot_r = size * 0.255
    ring_inner = size * 0.335
    ring_outer = size * 0.358
    rows: list[bytes] = []
    for py in range(size):
        row = bytearray()
        y = py + 0.5 - half
        for px in range(size):
            x = px + 0.5 - half
            dist = math.hypot(x, y)

            # --- tile -------------------------------------------------
            d_tile = _sd_round_rect(x, y, half - 1.0, radius)
            tile_alpha = _coverage(d_tile)
            if tile_alpha <= 0.0:
                row.extend((0, 0, 0, 0))
                continue

            # Vertical porcelain gradient.
            v = _clamp((py + 0.5) / size)
            colour = _mix(PORCELAIN_TOP, PORCELAIN_BOTTOM, v)

            # Soft inner highlight along the top edge.
            if y < 0 and -half < y:
                highlight = _clamp((abs(d_tile) - 1.0) / 26.0) * 0.55
                colour = _mix(colour, (255, 255, 255), highlight)

            # Border stroke, inside the tile edge.
            if -2.6 < d_tile < 0.0:
                edge = _clamp((d_tile + 2.6) / 2.6)
                colour = _mix(colour, BORDER, edge * 0.9)

            # --- macaron ring (a quiet echo of the palette) ------------
            ring_alpha = 0.0
            if ring_inner - 1.0 <= dist <= ring_outer + 1.0:
                a_in = _clamp((dist - (ring_inner - 1.0)) / 1.5)
                a_out = _clamp(((ring_outer + 1.0) - dist) / 1.5)
                ring_alpha = min(a_in, a_out) * 0.8
                angle = (math.atan2(y, x) / (2.0 * math.pi)) % 1.0
                ring_tint = _mix(MACARON_PINK, MACARON_LILAC, angle)
                colour = _mix(colour, ring_tint, ring_alpha)

            # --- accent dot -------------------------------------------
            if dist < dot_r + 1.5:
                dot_alpha = _clamp((dot_r + 1.5 - dist) / 2.0)
                t = _clamp(dist / dot_r)
                dot_colour = _mix(ACCENT, ACCENT_LIGHT, t * t)
                # A little strawberry kiss at the very edge of the dot.
                dot_colour = _mix(dot_colour, MACARON_PINK, _clamp((t - 0.72) / 0.28) * 0.55)
                # Top-left specular highlight.
                spec = math.hypot(x + dot_r * 0.32, y + dot_r * 0.32)
                if spec < dot_r * 0.55:
                    dot_colour = _mix(dot_colour, (255, 255, 255), _clamp(1.0 - spec / (dot_r * 0.55)) * 0.35)
                colour = _mix(colour, dot_colour, dot_alpha)
                # Re-assert the outer tile coverage (the dot is well inside).
                tile_alpha = min(tile_alpha, _clamp(tile_alpha))

            row.extend((colour[0], colour[1], colour[2], int(round(tile_alpha * 255))))
        rows.append(bytes(row))
    return b"".join(rows)


def write_png(path: Path, size: int, rgba: bytes) -> None:
    """Write a minimal, standards compliant 8-bit RGBA PNG."""
    stride = size * 4
    raw = bytearray()
    for y in range(size):
        raw.append(0)  # filter type: None
        raw.extend(rgba[y * stride : (y + 1) * stride])

    def chunk(kind: bytes, payload: bytes) -> bytes:
        data = kind + payload
        return struct.pack(">I", len(payload)) + data + struct.pack(">I", zlib.crc32(data) & 0xFFFFFFFF)

    png = bytearray(b"\x89PNG\r\n\x1a\n")
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(bytes(png))


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate EchoPort PNG icons (stdlib only).")
    parser.add_argument("--out-dir", default="assets", help="directory for icon.png (default: assets)")
    parser.add_argument("--size", type=int, action="append", default=None,
                        help="size(s) to render; default: 256 and 128")
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    out_dir = (root / args.out_dir).resolve() if not Path(args.out_dir).is_absolute() else Path(args.out_dir)
    sizes = args.size or [256, 128]

    for size in sizes:
        if size < 16 or size > 1024:
            raise SystemExit(f"unsupported icon size: {size}")
        name = "icon.png" if size == 256 else f"icon-{size}.png"
        path = out_dir / name
        write_png(path, size, render(size))
        print(f"wrote {path} ({size}x{size}, {path.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
