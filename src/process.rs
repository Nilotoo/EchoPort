//! Spawning external helper processes (yt-dlp, ffmpeg, …).
//!
//! On Windows every child process would otherwise allocate a console window
//! (a black flash for each search/download); `CREATE_NO_WINDOW` keeps the GUI
//! clean. All tool spawning in EchoPort must go through [`quiet`].

use std::ffi::OsStr;
use std::process::Command;

/// Build a `Command` that never flashes a console window on Windows.
pub fn quiet(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
