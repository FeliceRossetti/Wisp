//! Everything that talks to the page: the injected script, the URL policy and
//! the messages the page sends back.

use serde::Deserialize;
use url::Url;

pub const HOME: &str = "https://web.whatsapp.com/";
const HOST: &str = "web.whatsapp.com";

const INJECT_JS: &str = include_str!("web/inject.js");
const THEME_CSS: &str = include_str!("web/theme.css");
const PRIVACY_CSS: &str = include_str!("web/privacy.css");
const LOCK_CSS: &str = include_str!("web/lock.css");
const LOGO_PNG: &[u8] = include_bytes!("../assets/icons/wisp-128.png");

/// A WebKit user agent WhatsApp Web accepts. WKWebView and WebKitGTK don't
/// advertise a Safari version by default, so the site would call them unsupported.
#[cfg(not(windows))]
pub const WEBKIT_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
     (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

/// Messages posted by `inject.js` through `window.ipc.postMessage`.
///
/// The page can only *ask* to unlock; the app decides. PIN text is never logged.
#[derive(Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Ipc {
    Notify {
        title: String,
        #[serde(default)]
        body: String,
    },
    Quit,
    Hide,
    Theme {
        on: bool,
    },
    Privacy {
        on: bool,
    },
    Zoom {
        dir: ZoomDir,
    },
    /// The page finished loading and wants the current lock state.
    Ready,
    /// Lock now (Ctrl/Cmd+Shift+L).
    Lock,
    /// Ask the operating system to verify the user.
    Unlock,
    /// Try to unlock with a PIN.
    Pin {
        pin: String,
    },
    /// Save the PIN chosen while turning app lock on.
    SetPin {
        pin: String,
    },
    /// The user backed out of turning app lock on.
    CancelSetup,
    /// Show account number `to` (1-based; Ctrl/Cmd+1..9).
    Switch {
        to: usize,
    },
    /// Show the next or previous account (Ctrl+Tab / Ctrl+Shift+Tab).
    Cycle {
        forward: bool,
    },
}

impl std::fmt::Debug for Ipc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ipc::Pin { .. } => f.write_str("Pin { .. }"),
            Ipc::SetPin { .. } => f.write_str("SetPin { .. }"),
            Ipc::Notify { .. } => f.write_str("Notify { .. }"),
            other => write!(f, "{}", serde_json::to_string(&other.kind()).unwrap_or_default()),
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ZoomDir {
    In,
    Out,
    Reset,
}

impl Ipc {
    pub fn parse(body: &str) -> Option<Self> {
        serde_json::from_str(body).ok()
    }

    fn kind(&self) -> &'static str {
        match self {
            Ipc::Notify { .. } => "notify",
            Ipc::Quit => "quit",
            Ipc::Hide => "hide",
            Ipc::Theme { .. } => "theme",
            Ipc::Privacy { .. } => "privacy",
            Ipc::Zoom { .. } => "zoom",
            Ipc::Ready => "ready",
            Ipc::Lock => "lock",
            Ipc::Unlock => "unlock",
            Ipc::Pin { .. } => "pin",
            Ipc::SetPin { .. } => "setpin",
            Ipc::CancelSetup => "cancelsetup",
            Ipc::Switch { .. } => "switch",
            Ipc::Cycle { .. } => "cycle",
        }
    }
}

/// What the injected script needs to know when a page starts.
pub struct PageConfig<'a> {
    pub theme: bool,
    pub privacy: bool,
    pub accent: &'a str,
    pub user_css: &'a str,
    /// Start behind the lock screen (app lock is on).
    pub locked: bool,
    /// Name of the system unlock method ("Windows Hello"), or empty for PIN only.
    pub lock_system: &'a str,
}

/// Build the script injected into every document.
pub fn init_script(c: &PageConfig) -> String {
    let cfg = serde_json::json!({
        "theme": c.theme,
        "privacy": c.privacy,
        "accent": c.accent,
        "css": THEME_CSS,
        "privacyCss": PRIVACY_CSS,
        "userCss": c.user_css,
        "appLock": c.locked,
        "lockSystem": c.lock_system,
        "lockCss": LOCK_CSS,
        "logo": logo_data_url(),
    });
    INJECT_JS.replace("__WISP_CONFIG__", &cfg.to_string())
}

/// Script that shows the current lock state in the page.
pub fn lock_script(locked: bool, setup: bool, busy: bool, message: &str, system: &str) -> String {
    let state = serde_json::json!({
        "locked": locked,
        "setup": setup,
        "busy": busy,
        "message": message,
        "system": system,
    });
    format!("window.__wisp && window.__wisp.setLock({state})")
}

/// The Wisp logo as a data URL, for the lock screen and the account rail.
pub fn logo_data_url() -> String {
    format!("data:image/png;base64,{}", base64(LOGO_PNG))
}

/// The accent color if it's a plain CSS color (hex, a name or rgb()/hsl()),
/// otherwise the default teal. Keeps a hand-edited config from breaking out
/// of the stylesheet it's placed into.
pub fn css_color(color: &str) -> String {
    let c = color.trim();
    let plain = !c.is_empty()
        && c.len() <= 40
        && c.chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "#(),.% ".contains(ch));
    if plain {
        c.to_string()
    } else {
        "#06bcbc".to_string()
    }
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Whether the main frame may navigate to `url` inside the app.
pub fn is_internal(url: &str) -> bool {
    let Ok(u) = Url::parse(url) else { return false };
    match u.scheme() {
        "https" => u.host_str() == Some(HOST),
        "blob" | "data" | "about" => true,
        _ => false,
    }
}

/// Whether `url` is safe to hand to the system's default handler.
pub fn is_openable_externally(url: &str) -> bool {
    Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https" | "mailto" | "tel"))
}

/// Whether an IPC message came from WhatsApp Web itself (not some other frame).
pub fn is_trusted_origin(uri: &str) -> bool {
    Url::parse(uri).is_ok_and(|u| u.scheme() == "https" && u.host_str() == Some(HOST))
}

/// "(3) WhatsApp" -> 3, anything else -> 0.
pub fn unread_from_title(title: &str) -> u32 {
    let t = title.trim_start();
    t.strip_prefix('(')
        .and_then(|rest| rest.split_once(')'))
        .and_then(|(n, _)| n.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unread_count() {
        assert_eq!(unread_from_title("(3) WhatsApp"), 3);
        assert_eq!(unread_from_title("(120) WhatsApp Business"), 120);
        assert_eq!(unread_from_title("WhatsApp"), 0);
        assert_eq!(unread_from_title("(x) WhatsApp"), 0);
    }

    #[test]
    fn url_policy() {
        assert!(is_internal("https://web.whatsapp.com/"));
        assert!(is_internal("https://web.whatsapp.com/send?phone=1"));
        assert!(is_internal("blob:https://web.whatsapp.com/abc"));
        assert!(!is_internal("http://web.whatsapp.com/"));
        assert!(!is_internal("https://web.whatsapp.com.evil.test/"));
        assert!(!is_internal("https://example.com/"));
        assert!(is_openable_externally("https://example.com/"));
        assert!(is_openable_externally("mailto:a@b.c"));
        assert!(!is_openable_externally("file:///etc/passwd"));
        assert!(!is_openable_externally("javascript:alert(1)"));
        assert!(is_trusted_origin("https://web.whatsapp.com/"));
        assert!(!is_trusted_origin("https://static.whatsapp.net/"));
    }

    #[test]
    fn ipc_messages() {
        assert_eq!(
            Ipc::parse(r#"{"kind":"notify","title":"Ann","body":"hi"}"#),
            Some(Ipc::Notify {
                title: "Ann".into(),
                body: "hi".into()
            })
        );
        assert_eq!(
            Ipc::parse(r#"{"kind":"zoom","dir":"in"}"#),
            Some(Ipc::Zoom { dir: ZoomDir::In })
        );
        assert_eq!(Ipc::parse(r#"{"kind":"quit"}"#), Some(Ipc::Quit));
        assert_eq!(
            Ipc::parse(r#"{"kind":"privacy","on":true}"#),
            Some(Ipc::Privacy { on: true })
        );
        assert_eq!(Ipc::parse(r#"{"kind":"rm -rf"}"#), None);
    }

    #[test]
    fn accent_colors_are_sanitized() {
        assert_eq!(css_color("#22c55e"), "#22c55e");
        assert_eq!(css_color("rgb(10, 20, 30)"), "rgb(10, 20, 30)");
        assert_eq!(css_color("teal"), "teal");
        assert_eq!(css_color("red;}</style><script>"), "#06bcbc");
        assert_eq!(css_color(""), "#06bcbc");
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn lock_messages() {
        assert_eq!(Ipc::parse(r#"{"kind":"unlock"}"#), Some(Ipc::Unlock));
        assert_eq!(
            Ipc::parse(r#"{"kind":"pin","pin":"1234"}"#),
            Some(Ipc::Pin { pin: "1234".into() })
        );
        assert_eq!(Ipc::parse(r#"{"kind":"cancelsetup"}"#), Some(Ipc::CancelSetup));
        assert_eq!(
            Ipc::parse(r#"{"kind":"switch","to":2}"#),
            Some(Ipc::Switch { to: 2 })
        );
        assert_eq!(
            Ipc::parse(r#"{"kind":"cycle","forward":false}"#),
            Some(Ipc::Cycle { forward: false })
        );
        let debug = format!("{:?}", Ipc::Pin { pin: "1234".into() });
        assert!(!debug.contains("1234"), "PINs never show up in debug output");
        let js = lock_script(true, false, false, "Wrong PIN", "Windows Hello");
        assert!(js.starts_with("window.__wisp && window.__wisp.setLock({"));
    }

    #[test]
    fn script_has_config() {
        let s = init_script(&PageConfig {
            theme: true,
            privacy: false,
            accent: "#ff0000",
            user_css: "",
            locked: true,
            lock_system: "Windows Hello",
        });
        assert!(!s.contains("__WISP_CONFIG__"));
        assert!(s.contains("#ff0000"));
        assert!(s.contains("wisp-private"), "privacy CSS is embedded");
        assert!(s.contains("wisp-locked"), "lock CSS is embedded");
        assert!(
            s.contains("data:image/png;base64,iVBORw0KGgo"),
            "logo is embedded as PNG"
        );
    }
}
