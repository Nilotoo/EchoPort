# Release installers & packages

EchoPort has three release paths. All builds are unsigned unless stated otherwise and must be
verified on real hardware before a public announcement (the Linux artifacts are test-run in QA,
Windows/macOS artifacts are cross-built/CI-only in this round).

| Platform | Artifact | Built by | Notes |
|---|---|---|---|
| Linux (portable) | `echoport-<ver>-linux-x86_64.tar.gz` | `scripts/package-linux.sh` | binary + desktop + icons + docs/examples; extract & run |
| Arch Linux | `echoport-<ver>-1-x86_64.pkg.tar.zst` | `scripts/package-arch.sh` | pacman package; AUR recipe at `packaging/arch/PKGBUILD` |
| Windows | `echoport-<ver>-windows-x86_64.zip` | `scripts/package-windows-cross.sh` (CI: native `windows-latest`) | portable zip |
| Windows | `echoport-<ver>-windows-x86_64.msi` | `scripts/package-msi.sh` (msitools/wixl on Linux; CI Ubuntu job) | **per-user** install (`%LOCALAPPDATA%\EchoPort`), Start Menu shortcut, uninstall support; unsigned |
| macOS (CI-only) | `EchoPort.app.zip` | `scripts/package-macos.sh` (CI `macos-latest`) | universal aarch64+x86_64 `.app` with icon; ad-hoc unsigned, not notarized |

Every artifact gets a sibling `.sha256` file. The GitHub release workflow
(`packaging/ci/release-workflow.patch`, apply to `.github/workflows/release.yml`) builds the Linux
tarball + Arch package, Windows zip + MSI, the macOS zip and uploads all of them with checksums.

## Windows MSI details

`scripts/package-msi.sh` builds an unsigned MSI from the cross-built
`target/x86_64-pc-windows-gnu/release/echoport.exe`:

* per-user installation under `%LOCALAPPDATA%\EchoPort` (no administrator required);
* Start Menu shortcut “EchoPort”, ARP icon (new ringless `assets/echoport.ico`), uninstall support;
* installs exe + README-WINDOWS + LICENSE + README + docs (`SCRIPTING.md`, `PLUGINS.md`,
  `plugins.html`, `config.html`) + Lua config templates;
* validated on Linux with `msiinfo` tables/files/shortcuts; **not executed on Windows**.

Tooling: `wixl`/`msiinfo` from msitools. On Arch: `sudo pacman -S msitools`; without root the
scripts accept `WIXL_DIR` pointing at an extracted msitools tree.

## Arch pacman details

`scripts/package-arch.sh` assembles the package without root and prints `pacman -Qip` / `-Qlp`
validation; installs `/usr/bin/echoport`, `echoport.desktop`, hicolor 32/48/128/256 icons,
`/usr/share/doc/echoport/*` (README, LICENSE, docs, examples) and `/usr/share/echoport/VERSION`.
`packaging/arch/PKGBUILD` is the AUR-ready equivalent (set the published tarball sha256 before AUR
upload).

## macOS (CI-only)

`scripts/package-macos.sh` must run on macOS: it builds the `.app` (Info.plist, `.icns` via
sips/iconutil, docs/examples) and zips it. The CI patch adds a `macos-latest` job that builds both
`aarch64-apple-darwin` and `x86_64-apple-darwin` binaries and combines them with `lipo` into a
universal bundle. Locally (Linux) this path cannot be built or verified.
