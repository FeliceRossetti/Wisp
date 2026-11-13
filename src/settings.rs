//! The Settings screen: which settings exist, what values they accept, and
//! what the screen is told about the current state. Applying a change to the
//! running app happens in `main.rs`.

use crate::config::Config;
use serde::Deserialize;

/// Accent presets offered in Settings. Graphite is pure monochrome.
pub const ACCENTS: [(&str, &str); 6] = [
    ("Teal", "#06bcbc"),
    ("Blue", "#3b82f6"),
    ("Violet", "#8b5cf6"),
    ("Rose", "#f43f5e"),
    ("Amber", "#f59e0b"),
    ("Graphite", "#a1a1aa"),
];

/// "Lock after" choices, in minutes (0 = only on start or by hand).
pub const LOCK_AFTER: [u32; 6] = [0, 1, 5, 15, 30, 60];

/// One change made in Settings.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum Setting {
    Theme(bool),
    Accent(String),
    Zoom(f64),
    AccountBar(bool),
    PrivacyBlur(bool),
    Notifications(bool),
    NotificationPreview(bool),
    /// Only `false` is applied directly; turning app lock on asks for a PIN first.
    AppLock(bool),
    LockAfterMinutes(u32),
    CloseToTray(bool),
    StartHidden(bool),
    StartAtLogin(bool),
}

/// Accents are plain `#rrggbb` colors.
pub fn valid_accent(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Where Settings' About section may send people. Nothing else can be opened.
pub fn link(name: &str) -> Option<&'static str> {
    match name {
        "repo" => Some("https://github.com/FeliceRossetti/Wisp"),
        "releases" => Some("https://github.com/FeliceRossetti/Wisp/releases"),
        "issues" => Some("https://github.com/FeliceRossetti/Wisp/issues/new/choose"),
        "license" => Some("https://github.com/FeliceRossetti/Wisp/blob/main/LICENSE"),
        _ => None,
    }
}

/// What the Settings screen shows.
pub fn view(cfg: &Config, start_at_login: bool, system_unlock: Option<&str>) -> serde_json::Value {
    let accents: Vec<_> = ACCENTS
        .iter()
        .map(|(name, color)| serde_json::json!({ "name": name, "color": color }))
        .collect();
    serde_json::json!({
        "theme": cfg.theme_enabled,
        "accent": cfg.accent,
        "accents": accents,
        "zoom": cfg.zoom,
        "accountBar": cfg.account_bar,
        "privacyBlur": cfg.privacy_blur,
        "notifications": cfg.notifications,
        "notificationPreview": cfg.notification_preview,
        "appLock": cfg.app_lock,
        "lockAfter": cfg.lock_after_minutes,
        "lockAfterChoices": LOCK_AFTER,
        "systemUnlock": system_unlock,
        "closeToTray": cfg.close_to_tray,
        "startHidden": cfg.start_hidden,
        "startAtLogin": start_at_login,
        "system": if cfg!(windows) { "Windows" } else if cfg!(target_os = "macos") { "your Mac" } else { "your computer" },
        "version": env!("CARGO_PKG_VERSION"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_messages() {
        let parse = |s: &str| serde_json::from_str::<Setting>(s).ok();
        assert_eq!(
            parse(r#"{"name":"theme","value":false}"#),
            Some(Setting::Theme(false))
        );
        assert_eq!(
            parse(r##"{"name":"accent","value":"#3b82f6"}"##),
            Some(Setting::Accent("#3b82f6".into()))
        );
        assert_eq!(
            parse(r#"{"name":"lock_after_minutes","value":15}"#),
            Some(Setting::LockAfterMinutes(15))
        );
        assert_eq!(
            parse(r#"{"name":"start_at_login","value":true}"#),
            Some(Setting::StartAtLogin(true))
        );
        assert_eq!(
            parse(r#"{"name":"pin","value":"1234"}"#),
            None,
            "unknown settings are rejected"
        );
    }

    #[test]
    fn accents_and_links() {
        assert!(ACCENTS.iter().all(|(_, c)| valid_accent(c)));
        assert!(valid_accent("#ABCdef"));
        assert!(!valid_accent("teal"));
        assert!(!valid_accent("#12345"));
        assert!(!valid_accent("#12345g"));
        assert!(!valid_accent("#06bcbc; x"));
        assert_eq!(link("repo"), Some("https://github.com/FeliceRossetti/Wisp"));
        assert_eq!(link("https://evil.test"), None);
    }

    #[test]
    fn view_reflects_the_config() {
        let cfg = Config {
            lock_after_minutes: 15,
            ..Config::default()
        };
        let v = view(&cfg, true, Some("Windows Hello"));
        assert_eq!(v["lockAfter"], 15);
        assert_eq!(v["startAtLogin"], true);
        assert_eq!(v["systemUnlock"], "Windows Hello");
        assert_eq!(v["accents"].as_array().unwrap().len(), ACCENTS.len());
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    }
}
