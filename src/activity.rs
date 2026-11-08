//! Lets the webview rest when nobody is looking at it.
//!
//! - Window hidden (tray) or minimized: the page is marked hidden, so it is
//!   throttled like a background browser tab, and WebView2 is asked to use
//!   less memory. Messages and notifications still arrive.
//! - Window visible but unfocused for [`IDLE_AFTER`]: low memory only.
//! - Window focused: back to normal right away.

use std::time::{Duration, Instant};
use wry::WebView;

/// How long the window may sit unfocused before the webview trims its memory.
pub const IDLE_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Whether the page should be visible.
    pub visible: bool,
    /// Whether WebView2 should aim for low memory use.
    pub low_memory: bool,
    /// When to check again (the unfocused timer runs out).
    pub wake_at: Option<Instant>,
}

/// Decide the webview state from the window state.
pub fn plan(shown: bool, unfocused_since: Option<Instant>, now: Instant) -> Plan {
    if !shown {
        return Plan {
            visible: false,
            low_memory: true,
            wake_at: None,
        };
    }
    match unfocused_since {
        None => Plan {
            visible: true,
            low_memory: false,
            wake_at: None,
        },
        Some(since) => {
            let due = since + IDLE_AFTER;
            if now >= due {
                Plan {
                    visible: true,
                    low_memory: true,
                    wake_at: None,
                }
            } else {
                Plan {
                    visible: true,
                    low_memory: false,
                    wake_at: Some(due),
                }
            }
        }
    }
}

/// Applies plans to the webview, touching it only when something changes.
pub struct Activity {
    applied: Plan,
    unfocused_since: Option<Instant>,
}

impl Activity {
    pub fn new(focused: bool) -> Self {
        Self {
            applied: Plan {
                visible: true,
                low_memory: false,
                wake_at: None,
            },
            unfocused_since: if focused { None } else { Some(Instant::now()) },
        }
    }

    pub fn set_focused(&mut self, focused: bool) {
        match (focused, self.unfocused_since) {
            (true, _) => self.unfocused_since = None,
            (false, None) => self.unfocused_since = Some(Instant::now()),
            (false, Some(_)) => {}
        }
    }

    /// Bring the webview in line with the window; returns when to wake up next.
    pub fn update(&mut self, webview: &WebView, shown: bool) -> Option<Instant> {
        let want = plan(shown, self.unfocused_since, Instant::now());
        if want.visible != self.applied.visible {
            let _ = webview.set_visible(want.visible);
        }
        if want.low_memory != self.applied.low_memory {
            set_low_memory(webview, want.low_memory);
        }
        self.applied = want;
        want.wake_at
    }
}

#[cfg(windows)]
fn set_low_memory(webview: &WebView, low: bool) {
    use wry::{MemoryUsageLevel, WebViewExtWindows};
    let level = if low {
        MemoryUsageLevel::Low
    } else {
        MemoryUsageLevel::Normal
    };
    let _ = webview.set_memory_usage_level(level);
}

#[cfg(not(windows))]
fn set_low_memory(_webview: &WebView, _low: bool) {
    // WKWebView and WebKitGTK manage this themselves once the page is hidden.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_window_rests_fully() {
        let p = plan(false, None, Instant::now());
        assert_eq!(
            p,
            Plan {
                visible: false,
                low_memory: true,
                wake_at: None
            }
        );
    }

    #[test]
    fn focused_window_is_normal() {
        let p = plan(true, None, Instant::now());
        assert_eq!(
            p,
            Plan {
                visible: true,
                low_memory: false,
                wake_at: None
            }
        );
    }

    #[test]
    fn unfocused_window_trims_after_delay() {
        let t0 = Instant::now();
        let early = plan(true, Some(t0), t0 + Duration::from_secs(10));
        assert!(!early.low_memory && early.visible);
        assert_eq!(early.wake_at, Some(t0 + IDLE_AFTER));
        let late = plan(true, Some(t0), t0 + IDLE_AFTER);
        assert!(late.low_memory && late.visible);
        assert_eq!(late.wake_at, None);
    }
}
