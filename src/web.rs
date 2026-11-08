//! Everything that talks to the page: the injected script, the URL policy and
//! the messages the page sends back.

use serde::Deserialize;
use url::Url;

pub const HOME: &str = "https://web.whatsapp.com/";
const HOST: &str = "web.whatsapp.com";

const INJECT_JS: &str = include_str!("web/inject.js");
const THEME_CSS: &str = include_str!("web/theme.css");

/// A WebKit user agent WhatsApp Web accepts. WKWebView and WebKitGTK don't
/// advertise a Safari version by default, so the site would call them unsupported.
#[cfg(not(windows))]
pub const WEBKIT_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
     (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

/// Messages posted by `inject.js` through `window.ipc.postMessage`.
#[derive(Debug, Deserialize, PartialEq)]
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
    Zoom {
        dir: ZoomDir,
    },
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
}

/// Build the script injected into every document.
pub fn init_script(theme: bool, accent: &str, user_css: &str) -> String {
    let cfg = serde_json::json!({
        "theme": theme,
        "accent": accent,
        "css": THEME_CSS,
        "userCss": user_css,
    });
    INJECT_JS.replace("__WISP_CONFIG__", &cfg.to_string())
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
        assert_eq!(Ipc::parse(r#"{"kind":"rm -rf"}"#), None);
    }

    #[test]
    fn script_has_config() {
        let s = init_script(true, "#ff0000", "");
        assert!(!s.contains("__WISP_CONFIG__"));
        assert!(s.contains("#ff0000"));
    }
}
