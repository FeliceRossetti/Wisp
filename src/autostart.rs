//! "Open Wisp when you sign in". Wisp registers itself the way each system
//! expects for a per-user app, and starts quietly in the tray when launched
//! that way (the `--background` argument).
//!
//! - Windows: a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
//! - macOS: a launch agent in `~/Library/LaunchAgents`
//! - Linux: an entry in `~/.config/autostart` (XDG autostart)

use std::io;
use std::path::Path;

/// Passed when Wisp is started at sign-in.
pub const BACKGROUND_ARG: &str = "--background";

/// Whether Wisp starts when you sign in.
pub fn is_enabled() -> bool {
    platform::is_enabled()
}

/// Turn starting at sign-in on or off for the running copy of Wisp.
pub fn set_enabled(on: bool) -> io::Result<()> {
    if on {
        platform::enable(&std::env::current_exe()?)
    } else {
        platform::disable()
    }
}

/// The command line Windows runs at sign-in.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_command(exe: &Path) -> String {
    format!("\"{}\" {BACKGROUND_ARG}", exe.display())
}

/// The launch agent macOS runs at sign-in.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn launch_agent(label: &str, exe: &Path) -> String {
    let xml = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>{BACKGROUND_ARG}</string>
  </array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
        xml(label),
        xml(&exe.to_string_lossy())
    )
}

/// The XDG autostart entry Linux desktops run at sign-in.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn desktop_entry(exe: &Path) -> String {
    // Desktop entries quote arguments with double quotes and escape " ` $ \ inside them.
    let path = exe.to_string_lossy();
    let mut quoted = String::from("\"");
    for c in path.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    format!(
        "[Desktop Entry]\nType=Application\nName=Wisp\nComment=Start Wisp in the background when you sign in\nExec={quoted} {BACKGROUND_ARG}\nIcon=wisp\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(windows)]
mod platform {
    use std::io;
    use std::path::Path;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "Wisp";

    fn err(e: impl std::fmt::Display) -> io::Error {
        io::Error::other(e.to_string())
    }

    pub fn is_enabled() -> bool {
        windows_registry::CURRENT_USER
            .open(RUN_KEY)
            .and_then(|k| k.get_string(VALUE))
            .is_ok_and(|v| !v.trim().is_empty())
    }

    pub fn enable(exe: &Path) -> io::Result<()> {
        let key = windows_registry::CURRENT_USER.create(RUN_KEY).map_err(err)?;
        key.set_string(VALUE, super::windows_command(exe)).map_err(err)
    }

    pub fn disable() -> io::Result<()> {
        match windows_registry::CURRENT_USER.create(RUN_KEY) {
            Ok(key) if key.get_string(VALUE).is_ok() => key.remove_value(VALUE).map_err(err),
            Ok(_) => Ok(()),
            Err(e) => Err(err(e)),
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::io;
    use std::path::{Path, PathBuf};

    const LABEL: &str = crate::notify::APP_ID;

    fn plist() -> Option<PathBuf> {
        directories::BaseDirs::new().map(|d| {
            d.home_dir()
                .join("Library/LaunchAgents")
                .join(format!("{LABEL}.plist"))
        })
    }

    pub fn is_enabled() -> bool {
        plist().is_some_and(|p| p.exists())
    }

    pub fn enable(exe: &Path) -> io::Result<()> {
        let path = plist().ok_or_else(|| io::Error::other("no home folder"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, super::launch_agent(LABEL, exe))
    }

    pub fn disable() -> io::Result<()> {
        match plist() {
            Some(p) if p.exists() => std::fs::remove_file(p),
            _ => Ok(()),
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use std::io;
    use std::path::{Path, PathBuf};

    fn entry() -> Option<PathBuf> {
        directories::BaseDirs::new().map(|d| d.config_dir().join("autostart").join("wisp.desktop"))
    }

    pub fn is_enabled() -> bool {
        entry().is_some_and(|p| p.exists())
    }

    pub fn enable(exe: &Path) -> io::Result<()> {
        let path = entry().ok_or_else(|| io::Error::other("no home folder"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, super::desktop_entry(exe))
    }

    pub fn disable() -> io::Result<()> {
        match entry() {
            Some(p) if p.exists() => std::fs::remove_file(p),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_command_quotes_the_path() {
        let cmd = windows_command(Path::new(r"C:\Users\A B\AppData\Local\Programs\Wisp\Wisp.exe"));
        assert_eq!(
            cmd,
            r#""C:\Users\A B\AppData\Local\Programs\Wisp\Wisp.exe" --background"#
        );
    }

    #[test]
    fn launch_agent_escapes_xml() {
        let p = launch_agent(
            "io.example",
            Path::new("/Applications/A & <B>.app/Contents/MacOS/Wisp"),
        );
        assert!(p.contains("<string>/Applications/A &amp; &lt;B&gt;.app/Contents/MacOS/Wisp</string>"));
        assert!(p.contains("<string>--background</string>"));
        assert!(p.contains("<key>RunAtLoad</key><true/>"));
    }

    #[test]
    fn desktop_entry_quotes_and_escapes() {
        let e = desktop_entry(Path::new("/home/a b/$bin/wisp"));
        assert!(e.contains("Exec=\"/home/a b/\\$bin/wisp\" --background\n"), "{e}");
        assert!(e.starts_with("[Desktop Entry]\n"));
    }
}
