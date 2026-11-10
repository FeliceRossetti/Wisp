//! Desktop notifications, sent under Wisp's own name and icon.
//!
//! On Windows a notification is labelled with the app that sent it. An app that
//! isn't installed from the Store needs an identity (an AppUserModelID), and
//! Windows only shows its pop-ups once a Start menu shortcut carries that
//! identity. Each time Wisp starts it registers its name and icon for the
//! current user and makes sure its Start menu shortcut carries the identity,
//! which also keeps both right after Wisp moves or updates.

use std::path::Path;

/// Wisp's app identity, the same as the macOS bundle id. Linux notification
/// servers go by the app name instead, so it isn't used there.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub const APP_ID: &str = "io.github.felicerossetti.wisp";

/// Run once at startup, before any window opens. `data_dir` is Wisp's local
/// data folder, where the notification icon is kept.
pub fn init(data_dir: &Path) {
    #[cfg(windows)]
    {
        if let Err(e) = windows::register(data_dir) {
            eprintln!("notifications: could not register Wisp ({e})");
        }
        windows::set_process_identity();
        if let Ok(exe) = std::env::current_exe() {
            // Release builds may create the Start menu entry Windows needs; test
            // builds only update an existing one, so they never add entries.
            if let Err(e) = windows::ensure_shortcuts(&exe, !cfg!(debug_assertions)) {
                eprintln!("notifications: could not update the Start menu shortcut ({e})");
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = data_dir;
        // Attribute notifications to Wisp.app instead of the terminal that started it.
        let _ = notify_rust::set_application(APP_ID);
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = data_dir;
}

/// Show a notification. `on_click` runs (on another thread) if it's clicked
/// while Wisp is running.
pub fn show(title: String, body: String, on_click: impl Fn() + Send + 'static) {
    // Delivering a notification can block for a moment; keep the UI responsive.
    std::thread::spawn(move || {
        #[cfg(windows)]
        windows::show(&title, &body, on_click);
        #[cfg(not(windows))]
        {
            let _ = on_click;
            let mut n = notify_rust::Notification::new();
            n.summary(&title).appname("Wisp");
            if !body.is_empty() {
                n.body(&body);
            }
            if let Err(e) = n.show() {
                eprintln!("notification failed: {e}");
            }
        }
    });
}

#[cfg(windows)]
mod windows {
    use super::APP_ID;
    use std::path::Path;
    use std::time::Duration;
    use tauri_winrt_notification::Toast;

    const ICON_PNG: &[u8] = include_bytes!("../assets/icons/wisp-256.png");

    /// The registry key that gives Wisp's notifications their name and icon.
    pub fn key_path() -> String {
        format!(r"Software\Classes\AppUserModelId\{APP_ID}")
    }

    pub fn register(data_dir: &Path) -> windows_registry::Result<()> {
        let icon = data_dir.join("notification-icon.png");
        if std::fs::read(&icon).ok().as_deref() != Some(ICON_PNG) {
            let _ = std::fs::create_dir_all(data_dir);
            let _ = std::fs::write(&icon, ICON_PNG);
        }
        let key = windows_registry::CURRENT_USER.create(key_path())?;
        key.set_string("DisplayName", "Wisp")?;
        key.set_string("IconUri", icon.to_string_lossy())?;
        Ok(())
    }

    /// Group Wisp's windows (and a pinned taskbar shortcut) under its identity.
    pub fn set_process_identity() {
        use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
        let _ = unsafe { SetCurrentProcessExplicitAppUserModelID(&windows::core::HSTRING::from(APP_ID)) };
    }

    /// Give Wisp's Start menu shortcut (and its desktop one) the app identity.
    /// Only shortcuts that point to this very program are changed, so a test
    /// build never takes over the installed copy's shortcuts. With `create`, a
    /// missing Start menu shortcut is added.
    pub fn ensure_shortcuts(exe: &Path, create: bool) -> windows::core::Result<()> {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // The UI thread already uses COM; this just makes sure it's initialized.
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let start_menu = std::env::var_os("APPDATA")
            .map(|d| Path::new(&d).join(r"Microsoft\Windows\Start Menu\Programs\Wisp.lnk"));
        let desktop = directories::UserDirs::new().and_then(|u| u.desktop_dir().map(|d| d.join("Wisp.lnk")));
        if let Some(link) = start_menu {
            unsafe { tag_shortcut(&link, exe, create)? };
        }
        if let Some(link) = desktop {
            let _ = unsafe { tag_shortcut(&link, exe, false) };
        }
        Ok(())
    }

    unsafe fn tag_shortcut(link: &Path, exe: &Path, create: bool) -> windows::core::Result<()> {
        use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
        use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
        use windows::Win32::System::Com::{
            CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READWRITE,
        };
        use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
        use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
        use windows::core::{BSTR, HSTRING, Interface};

        let shell_link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
        let file: IPersistFile = shell_link.cast()?;
        let link_path = HSTRING::from(link.as_os_str());
        let exists = link.exists();
        if exists {
            unsafe { file.Load(&link_path, STGM_READWRITE)? };
            let mut target = [0u16; 1024];
            unsafe { shell_link.GetPath(&mut target, std::ptr::null_mut(), 0)? };
            let len = target.iter().position(|&c| c == 0).unwrap_or(target.len());
            if !same_file(Path::new(&String::from_utf16_lossy(&target[..len])), exe) {
                return Ok(());
            }
        } else if create {
            let exe_path = HSTRING::from(exe.as_os_str());
            unsafe {
                shell_link.SetPath(&exe_path)?;
                shell_link.SetIconLocation(&exe_path, 0)?;
                if let Some(dir) = exe.parent() {
                    shell_link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
                }
                shell_link.SetDescription(&HSTRING::from("A lightweight desktop app for WhatsApp Web"))?;
            }
        } else {
            return Ok(());
        }

        let store: IPropertyStore = shell_link.cast()?;
        let current = unsafe { store.GetValue(&PKEY_AppUserModel_ID) }
            .ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string());
        if exists && current.as_deref() == Some(APP_ID) {
            return Ok(());
        }
        unsafe {
            store.SetValue(&PKEY_AppUserModel_ID, &PROPVARIANT::from(APP_ID))?;
            store.Commit()?;
            file.Save(&link_path, true)
        }
    }

    fn same_file(a: &Path, b: &Path) -> bool {
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => a.as_os_str().eq_ignore_ascii_case(b.as_os_str()),
        }
    }

    pub fn show(title: &str, body: &str, on_click: impl Fn() + Send + 'static) {
        let mut toast = Toast::new(APP_ID).title(title).on_activated(move |_| {
            on_click();
            Ok(())
        });
        if !body.is_empty() {
            toast = toast.text1(body);
        }
        if let Err(e) = toast.show() {
            eprintln!("notification failed: {e}");
            return;
        }
        // Clicks are reported to this toast object, so keep it alive while the
        // notification is likely still on screen.
        std::thread::sleep(Duration::from_secs(60));
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn key_is_per_user_classes() {
            assert_eq!(
                super::key_path(),
                r"Software\Classes\AppUserModelId\io.github.felicerossetti.wisp"
            );
        }
    }
}
