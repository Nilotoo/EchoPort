//! Embed the application icon into Windows executables when a resource
//! compiler is available. Linux/macOS builds are untouched.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/echoport.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Only the GNU toolchain has a windres on the cross-build hosts we use;
    // MSVC builds keep the generated window icon (docs explain how to add one).
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu") {
        return;
    }
    let ico = Path::new("assets/echoport.ico");
    if !ico.is_file() {
        println!("cargo:warning=assets/echoport.ico not found; building without exe icon");
        return;
    }
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let rc_path = out_dir.join("echoport.rc");
    let obj_path = out_dir.join("echoport_icon.o");
    let ico_abs = std::fs::canonicalize(ico).unwrap_or_else(|_| ico.to_path_buf());
    let escaped = ico_abs.display().to_string().replace('\\', "\\\\");
    if std::fs::write(&rc_path, format!("1 ICON \"{escaped}\"\n")).is_err() {
        println!("cargo:warning=could not write resource script; building without exe icon");
        return;
    }
    let candidates = [
        std::env::var("WINDRES").ok().map(PathBuf::from),
        Some(PathBuf::from("x86_64-w64-mingw32-windres")),
        Some(PathBuf::from("llvm-windres")),
        Some(PathBuf::from("windres")),
    ];
    for candidate in candidates.into_iter().flatten() {
        let status = Command::new(&candidate)
            .arg(&rc_path)
            .arg("-O")
            .arg("coff")
            .arg("-o")
            .arg(&obj_path)
            .status();
        if matches!(&status, Ok(s) if s.success()) && obj_path.is_file() {
            println!("cargo:rustc-link-arg={}", obj_path.display());
            return;
        }
    }
    println!("cargo:warning=windres not found; building Windows exe without embedded icon");
}
