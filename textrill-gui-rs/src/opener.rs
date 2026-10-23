//! Hand a path or URL to the desktop's default handler.
//!
//! This is the only local-launch surface in the app. It never fetches anything:
//! the desktop's own opener starts the browser, file manager or registered
//! application, exactly as if the user had launched it. The platform command is
//! chosen at compile time so the same call works in the Linux, Windows and
//! macOS builds.

use std::ffi::OsStr;
use std::process::Child;

/// Open `target` with the platform's default handler.
///
/// `target` is a filesystem path or a URL. The caller reports a failure as a
/// status message; this helper does not decide how loud it should be.
pub fn open_target(target: impl AsRef<OsStr>) -> std::io::Result<Child> {
    let target = target.as_ref();

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(target).spawn()
    }

    #[cfg(target_os = "windows")]
    {
        // `start` is a cmd built-in. The empty title argument keeps a quoted
        // target from being mistaken for the window title.
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(target)
            .spawn()
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open").arg(target).spawn()
    }
}
