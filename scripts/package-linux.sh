#!/usr/bin/env bash
# EchoPort — build the Linux release tarball.
#
#   ./scripts/package-linux.sh
#
# Result (also printed at the end):
#   dist/echoport-<version>-linux-x86_64/
#       echoport                 release binary (stripped, LTO, panic=abort)
#       echoport.desktop         XDG desktop entry
#       echoport.png             256x256 application icon
#       icon-128.png             optional small icon
#       README.md                bilingual readme
#       LICENSE                  GPL-3.0 (when present)
#   dist/echoport-<version>-linux-x86_64.tar.gz
#   dist/echoport-<version>-linux-x86_64.tar.gz.sha256
#
# Safe to re-run: the staging directory and tarball are recreated, the shared
# cargo target directory is reused and never deleted.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() { echo "error: $*" >&2; exit 1; }

# ---- version --------------------------------------------------------------
VERSION=""
if command -v cargo >/dev/null 2>&1; then
    VERSION="$(cargo metadata --no-deps --format-version 1 2>/dev/null \
        | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])' 2>/dev/null || true)"
fi
if [ -z "$VERSION" ]; then
    VERSION="$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)"
fi
[ -n "$VERSION" ] || fail "could not determine the package version from Cargo.toml/cargo metadata"

# ---- inputs ---------------------------------------------------------------
[ -f "packaging/echoport.desktop" ] || fail "missing packaging/echoport.desktop"
[ -f "assets/icon.png" ] || { echo "assets/icon.png missing; generating it…"; python3 scripts/gen-icon.py; }
[ -f "assets/icon.png" ] || fail "missing assets/icon.png (run scripts/gen-icon.py)"
[ -f "README.md" ] || fail "missing README.md (run from the repository root)"

# ---- build ----------------------------------------------------------------
echo "==> building release binary"
./scripts/build.sh --release
BIN="target/release/echoport"
[ -x "$BIN" ] || fail "release binary not found at $BIN"

# ---- stage ----------------------------------------------------------------
NAME="echoport-${VERSION}-linux-x86_64"
STAGE="dist/${NAME}"
TARBALL="dist/${NAME}.tar.gz"
echo "==> staging $STAGE"
rm -rf "$STAGE"
rm -f "$TARBALL" "$TARBALL.sha256"
mkdir -p "$STAGE"

install -m 0755 "$BIN" "$STAGE/echoport"
install -m 0644 "packaging/echoport.desktop" "$STAGE/echoport.desktop"
install -m 0644 "assets/icon.png" "$STAGE/echoport.png"
for icon in assets/icon-32.png assets/icon-48.png assets/icon-128.png; do
    [ -f "$icon" ] && install -m 0644 "$icon" "$STAGE/$(basename "$icon")"
done
if [ -f "scripts/install-arch.sh" ]; then
    install -m 0755 "scripts/install-arch.sh" "$STAGE/install-arch.sh"
fi
install -m 0644 "README.md" "$STAGE/README.md"
if [ -f VERSION ]; then install -m 0644 VERSION "$STAGE/VERSION"; fi
if [ -f "LICENSE" ]; then
    install -m 0644 "LICENSE" "$STAGE/LICENSE"
fi
if [ -f "NOTICE" ]; then
    install -m 0644 "NOTICE" "$STAGE/NOTICE"
fi
# User-facing docs for the optional Lua scripting / plugin layer.
for doc in docs/SCRIPTING.md docs/PLUGINS.md docs/plugins.html docs/config.html; do
    if [ -f "$doc" ]; then
        mkdir -p "$STAGE/docs"
        install -m 0644 "$doc" "$STAGE/docs/"
    fi
done
# In-app icon-set licence/credits (assets are embedded in the binary; MIT-0).
if [ -f "assets/icons/LICENSE.txt" ]; then
    mkdir -p "$STAGE/assets/icons"
    install -m 0644 "assets/icons/LICENSE.txt" "$STAGE/assets/icons/LICENSE.txt"
    if [ -f "assets/icons/README.md" ]; then
        install -m 0644 "assets/icons/README.md" "$STAGE/assets/icons/README.md"
    fi
fi
# Ready-to-copy Lua templates (config + example plugin).
if [ -f "examples/config.lua" ]; then
    mkdir -p "$STAGE/examples/plugins"
    install -m 0644 "examples/config.lua" "$STAGE/examples/config.lua"
    [ -f "examples/config.zh-CN.lua" ] && install -m 0644 "examples/config.zh-CN.lua" "$STAGE/examples/config.zh-CN.lua"
    [ -f "examples/config.en.lua" ] && install -m 0644 "examples/config.en.lua" "$STAGE/examples/config.en.lua"
    if [ -f "examples/plugins/example.lua" ]; then
        install -m 0644 "examples/plugins/example.lua" "$STAGE/examples/plugins/example.lua"
    fi
fi

cat > "$STAGE/INSTALL.txt" <<'EOF'
EchoPort — Linux tarball
========================

Quick install (per-user, no root; Arch: ./install-arch.sh does all of this):

  mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/{32x32,48x48,128x128,256x256}/apps
  install -m 0755 echoport          ~/.local/bin/echoport
  install -m 0644 echoport.desktop  ~/.local/share/applications/echoport.desktop
  install -m 0644 echoport.png      ~/.local/share/icons/hicolor/256x256/apps/echoport.png
  install -m 0644 icon-128.png      ~/.local/share/icons/hicolor/128x128/apps/echoport.png
  install -m 0644 icon-48.png       ~/.local/share/icons/hicolor/48x48/apps/echoport.png
  install -m 0644 icon-32.png       ~/.local/share/icons/hicolor/32x32/apps/echoport.png
  install -m 0644 echoport.png     ~/.local/share/icons/hicolor/256x256/apps/echoport.png
  # (re)build the icon cache if your desktop needs it:
  gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor 2>/dev/null || true
  update-desktop-database ~/.local/share/applications 2>/dev/null || true

System-wide install: use /usr/local/bin and /usr/share/{applications,icons/hicolor/256x256/apps}.

Requirements:
  * A desktop with OpenGL (X11 or Wayland; software GL works).
  * yt-dlp and ffmpeg on PATH for online playback and downloads.
      - Arch:        sudo pacman -S yt-dlp ffmpeg
      - Debian/Ubuntu: sudo apt install yt-dlp ffmpeg
      - Fedora:      sudo dnf install yt-dlp ffmpeg
    (yt-dlp is best kept up to date: `yt-dlp -U` or your package manager.)
  * A CJK font for Chinese text: noto-fonts-cjk / fonts-noto-cjk.

Run:  echoport

Optional Lua configuration / plugins (see docs/SCRIPTING.md, docs/PLUGINS.md):
  mkdir -p ~/.config/echoport/plugins
  cp examples/config.lua            ~/.config/echoport/config.lua   # reference config
  cp examples/plugins/example.lua   ~/.config/echoport/plugins/     # example plugin
  # then Settings -> Reload scripts (the app also writes a commented
  # ~/.config/echoport/config.lua template on first run if none exists).
Data: $XDG_DATA_HOME/echoport (default ~/.local/share/echoport)
Cache: $XDG_CACHE_HOME/echoport (default ~/.cache/echoport)
EOF

# ---- archive --------------------------------------------------------------
echo "==> creating tarball"
tar -czf "$TARBALL" -C dist "$NAME"
SHA="$(sha256sum "$TARBALL" | cut -d' ' -f1)"
printf '%s  %s\n' "$SHA" "$(basename "$TARBALL")" > "$TARBALL.sha256"

echo
echo "packaged: $ROOT/$TARBALL"
echo "  size:    $(du -h "$TARBALL" | cut -f1)"
echo "  sha256:  $SHA"
echo "  contents:"
tar -tzf "$TARBALL" | sed 's/^/    /'
