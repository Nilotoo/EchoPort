# EchoPort — Windows build / packaging helper (PowerShell 5.1+ / PowerShell 7).
#
# There are two supported ways to get the Windows binary:
#
#   A. Build natively on Windows (recommended, fully supported):
#        rustup default stable-msvc        # Rust 1.85+ (VS Build Tools required)
#        powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
#
#   B. Cross-compile from Linux and copy the .exe over (best effort):
#        rustup target add x86_64-pc-windows-gnu
#        sudo pacman -S mingw-w64-gcc      # Debian: gcc-mingw-w64-x86-64
#        cargo build --release --target x86_64-pc-windows-gnu
#      Then copy target/x86_64-pc-windows-gnu/release/echoport.exe to Windows.
#      This script cannot run on Linux; it is for packaging on Windows.
#
# Native prerequisites (Windows):
#   * Rust stable (MSVC toolchain) + Visual Studio Build Tools ("Desktop
#     development with C++"). Rust 1.85+.
#   * yt-dlp.exe and ffmpeg.exe on PATH, or next to echoport.exe. EchoPort
#     auto-detects them; they are optional (local files still play without).
#   * Optional: a CJK font (Microsoft YaHei ships with Windows).
#   * Runtime: OpenGL 3.3+ driver; unsigned exe triggers SmartScreen (More info → Run anyway).
#
# Output:
#   dist\echoport-<version>-windows-x86_64\
#       echoport.exe
#       README-WINDOWS.txt    (SmartScreen, data dirs, yt-dlp.exe/ffmpeg.exe)
#       echoport.ico          (multi-size icon for shortcuts)
#       echoport.desktop.txt  (reference copy; Windows itself uses the .exe)
#       echoport.png
#       README.md / LICENSE / docs / examples
#   dist\echoport-<version>-windows-x86_64.zip
#
# Safe to re-run: the staging folder and zip are recreated. Cargo's target
# directory is reused, never deleted.
#
# Notes on the release profile: Cargo.toml sets strip=true, LTO=thin,
# codegen-units=1 and panic="abort"; the .exe is a single self-contained
# executable with no installer and no external runtime besides the C runtime.
# Bundling yt-dlp.exe/ffmpeg.exe inside the zip is allowed (they are separate
# GPL/LGPL-licensed programs) but not done automatically: get them from
# https://github.com/yt-dlp/yt-dlp/releases and https://ffmpeg.org/download.html.

[CmdletBinding()]
param(
    # "native" builds target\release on this machine; "cross" explains the
    # Linux cross-compile invocation and exits.
    [ValidateSet("native", "cross")]
    [string]$Target = "native",

    # Skip zipping (useful to inspect the staging folder).
    [switch]$NoArchive
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

if ($Target -eq "cross") {
    Write-Host "EchoPort — cross-compiling from Linux (best effort, unsupported for releases):"
    Write-Host ""
    Write-Host "  rustup target add x86_64-pc-windows-gnu"
    Write-Host "  sudo pacman -S mingw-w64-gcc          # Debian: gcc-mingw-w64-x86-64"
    Write-Host "  cargo build --release --target x86_64-pc-windows-gnu"
    Write-Host ""
    Write-Host "Result: target/x86_64-pc-windows-gnu/release/echoport.exe"
    Write-Host "A pure-Rust toolchain without mingw-w64 headers/libraries cannot build the"
    Write-Host "vendored Lua C code; with mingw-w64 installed the command above works."
    Write-Host "A ready-to-run Linux helper is included: scripts/package-windows-cross.sh"
    Write-Host "Official Windows packages must be built natively on Windows (this script)."
    exit 0
}

# ---- prerequisites ---------------------------------------------------------
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    throw "cargo was not found. Install Rust from https://rustup.rs (Rust 1.85+)."
}
Write-Host "EchoPort Windows build"
Write-Host ("  " + (& cargo --version))

$TargetTriple = (& rustc -vV | Select-String '^host: ' | ForEach-Object { $_.ToString().Replace('host: ', '').Trim() })
if ($TargetTriple -notlike "*windows*") {
    throw "This script must run on Windows (detected host: $TargetTriple). Use -Target cross for the Linux cross-compile notes."
}

# ---- version ---------------------------------------------------------------
$version = $null
$metaJson = & cargo metadata --no-deps --format-version 1 2>$null
if ($LASTEXITCODE -eq 0 -and $metaJson) {
    try {
        $version = ($metaJson | ConvertFrom-Json).packages[0].version
    } catch { $version = $null }
}
if (-not $version) {
    $version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
}
if (-not $version) { throw "could not determine the package version from Cargo.toml" }

# ---- build -----------------------------------------------------------------
Write-Host "==> cargo build --release"
& cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed (exit $LASTEXITCODE)" }

$exe = Join-Path $Root "target\release\echoport.exe"
if (-not (Test-Path $exe)) { throw "expected binary not found: $exe" }
$sizeMb = [math]::Round((Get-Item $exe).Length / 1MB, 2)

# ---- stage -----------------------------------------------------------------
$name = "echoport-$version-windows-x86_64"
$stage = Join-Path $Root "dist\$name"
$zip = Join-Path $Root "dist\$name.zip"
Write-Host "==> staging dist\$name"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
if (Test-Path $zip) { Remove-Item -Force $zip }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Copy-Item $exe (Join-Path $stage "echoport.exe")

# Runtime resources: icon (shortcut/manual use), docs, examples, licences.
$files = @(
    "assets\echoport.ico",
    "assets\icon.png",
    "README.md",
    "LICENSE",
    "packaging\README-WINDOWS.md",
    "packaging\echoport.desktop",
    "docs\SCRIPTING.md",
    "docs\PLUGINS.md",
    "docs\plugins.html",
    "docs\config.html",
    "examples\config.lua",
    "examples\config.zh-CN.lua",
    "examples\config.en.lua",
    "examples\plugins\example.lua"
)
foreach ($asset in $files) {
    $src = Join-Path $Root $asset
    if (Test-Path $src) {
        $destName = switch -Wildcard ($asset) {
            "*README-WINDOWS.md" { "README-WINDOWS.txt" }
            "*desktop"           { "echoport.desktop.txt" }
            default              { Split-Path -Leaf $asset }
        }
        $destDir = Join-Path $stage (Split-Path -Parent $destName)
        if ((Split-Path -Parent $destName) -and (Split-Path -Parent $destName) -ne ".") {
            New-Item -ItemType Directory -Force -Path $destDir | Out-Null
        }
        Copy-Item $src (Join-Path $stage $destName)
    }
}

# Windows-specific README is generated from packaging\README-WINDOWS.md above,
# with the package banner + version appended.
$banner = @"

==========================================================
EchoPort v$version — Windows x86_64 package (unsigned)
Built: $(Get-Date -Format "yyyy-MM-dd HH:mm")
Package: dist\$name.zip
==========================================================
"@
Add-Content -Encoding UTF8 (Join-Path $stage "README-WINDOWS.txt") $banner

if (-not $NoArchive) {
    Write-Host "==> creating dist\$name.zip"
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal
}

Write-Host ""
Write-Host "built:   $exe ($sizeMb MB)"
Write-Host "staged:  $stage"
if (-not $NoArchive) {
    Write-Host ("archive: " + $zip + " (" + [math]::Round((Get-Item $zip).Length / 1MB, 2) + " MB)")
}
Write-Host ""
Write-Host "Note: the Windows .exe is validated by building natively on Windows;"
Write-Host "macOS users build from source with 'cargo build --release' (see README.md)."
