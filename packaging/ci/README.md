# CI release workflow patch

`.github/**` is owned by the Lead, so this directory ships an exact patch instead of editing the
workflow directly.

Apply it after the v0.2.0 freeze / before pushing the public repo:

```bash
git apply packaging/ci/release-workflow.patch
```

What it adds on top of the current workflow:

* **linux job**: also builds `echoport-<ver>-1-x86_64.pkg.tar.zst` (Arch) via
  `scripts/package-arch.sh` and uploads it with the tarball + sha256.
* **windows-msi job** (ubuntu-latest): installs `mingw-w64` + `msitools`, cross-builds the Windows
  binary and runs `scripts/package-msi.sh`, uploading the MSI + sha256.
* **macos job** (macos-latest): builds `aarch64-apple-darwin` + `x86_64-apple-darwin`, `lipo`s the
  universal binary, runs `scripts/package-macos.sh` and uploads `EchoPort.app.zip` + sha256.
* **release job**: now waits for linux, windows, windows-msi and macos and publishes every
  `artifacts/**/*` file (all builds + `.sha256`).

`release-workflow-v2.patch` is an optional follow-up that switches the Linux job to
`scripts/package-linux.sh` so the CI tarball is byte-for-byte the same recipe as the local one
(VERSION file, 32/48/128/256 icons, `install-arch.sh`, checksums) instead of the older inline copy.
Apply it after the first patch and validate the YAML.

Both patches were checked with `git apply --check` and the resulting YAML parses. macOS output and the
MSI import on a real Windows machine remain unverified in this environment.
