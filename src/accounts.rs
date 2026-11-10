//! Several WhatsApp accounts side by side. Each account has its own webview and
//! its own data folder, so sessions never mix. Nothing here touches a window;
//! `main.rs` turns these decisions into webviews.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The id of the account that existed before multiple accounts; it keeps the
/// original data folder so upgrading never signs anyone out.
pub const DEFAULT_ID: &str = "default";

/// Width of the account rail, in logical pixels, when there are 2+ accounts.
pub const RAIL_WIDTH: f64 = 64.0;

/// Most accounts Wisp offers (each one is a full WhatsApp Web session).
pub const MAX_ACCOUNTS: usize = 9;

const PALETTE: [&str; 8] = [
    "#06bcbc", "#0892da", "#7c5cff", "#e8578c", "#f59e0b", "#22a06b", "#ef5350", "#64748b",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub color: String,
}

impl Account {
    pub fn initials(&self) -> String {
        initials(&self.name)
    }
}

/// The list Wisp starts with: whatever is saved, or the one original account.
pub fn normalize(accounts: &mut Vec<Account>, active: &mut String) {
    accounts.retain(|a| valid_id(&a.id));
    let mut seen = std::collections::HashSet::new();
    accounts.retain(|a| seen.insert(a.id.clone()));
    accounts.truncate(MAX_ACCOUNTS);
    if accounts.is_empty() {
        accounts.push(Account {
            id: DEFAULT_ID.into(),
            name: "Personal".into(),
            color: PALETTE[0].into(),
        });
    }
    for a in accounts.iter_mut() {
        a.name = clean_name(&a.name).unwrap_or_else(|| "Account".into());
    }
    if !accounts.iter().any(|a| a.id == *active) {
        *active = accounts[0].id.clone();
    }
}

/// A new account with a fresh id, a default name and an unused color.
pub fn new_account(existing: &[Account]) -> Account {
    let mut bytes = [0u8; 6];
    getrandom::fill(&mut bytes).expect("the system random number generator works");
    let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let n = (1..)
        .find(|n| !existing.iter().any(|a| a.name == format!("Account {n}")))
        .unwrap_or(1);
    let color = PALETTE
        .iter()
        .find(|c| !existing.iter().any(|a| a.color == **c))
        .unwrap_or(&PALETTE[existing.len() % PALETTE.len()]);
    Account {
        id,
        name: format!("Account {n}"),
        color: (*color).into(),
    }
}

/// Names are trimmed, single-line and at most 32 characters.
pub fn clean_name(name: &str) -> Option<String> {
    let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let name: String = name.chars().filter(|c| !c.is_control()).take(32).collect();
    (!name.is_empty()).then_some(name)
}

/// Up to two letters: first letters of the first two words, or of the name.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let letters: String = match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().filter(|c| c.is_alphanumeric()).take(2).collect(),
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect(),
    };
    if letters.is_empty() {
        "?".into()
    } else {
        letters.to_uppercase()
    }
}

/// Ids are lowercase hex (or the original "default"), so they're safe in paths.
fn valid_id(id: &str) -> bool {
    id == DEFAULT_ID || (!id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Where an account keeps its session. The original account keeps the folder
/// it always had; others get `webview-<id>` next to it.
pub fn data_dir(default_dir: &Path, id: &str) -> PathBuf {
    if id == DEFAULT_ID {
        default_dir.to_path_buf()
    } else {
        default_dir.with_file_name(format!("webview-{id}"))
    }
}

/// Session folders left behind by removed accounts (they can't always be
/// deleted while the webview is still shutting down).
pub fn orphaned_dirs(default_dir: &Path, accounts: &[Account]) -> Vec<PathBuf> {
    let Some(parent) = default_dir.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let id = name.strip_prefix("webview-")?;
            (valid_id(id) && id != DEFAULT_ID && !accounts.iter().any(|a| a.id == id)).then(|| e.path())
        })
        .collect()
}

/// macOS keeps sessions apart by a 16-byte data store id.
#[cfg(target_os = "macos")]
pub fn store_id(id: &str) -> [u8; 16] {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(format!("wisp-account:{id}").as_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

/// Whether the account bar shows: always with 2+ accounts, and with one
/// account unless the user turned that off.
pub fn show_bar(accounts: usize, with_one: bool) -> bool {
    accounts > 1 || with_one
}

/// Where the rail and the account pages go in a window `width` logical pixels
/// wide: `(rail width, page x, page width)`.
pub fn layout(bar: bool, width: f64) -> (f64, f64, f64) {
    let rail = if bar { RAIL_WIDTH } else { 0.0 };
    (rail, rail, (width - rail).max(0.0))
}

/// Notification title, naming the account when there's more than one.
pub fn notification_title(account: &str, title: &str, many: bool) -> String {
    if many {
        format!("{account} · {title}")
    } else {
        title.to_string()
    }
}

/// Messages from the account rail page (our own page, never WhatsApp's).
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RailMsg {
    Ready,
    Select {
        id: String,
    },
    Add,
    Manage {
        open: bool,
    },
    Rename {
        id: String,
        name: String,
    },
    Move {
        id: String,
        up: bool,
    },
    Remove {
        id: String,
    },
    /// Show the bar even with a single account.
    Bar {
        with_one: bool,
    },
}

impl RailMsg {
    pub fn parse(body: &str) -> Option<Self> {
        serde_json::from_str(body).ok()
    }
}

/// One account as the rail draws it.
pub struct RailItem<'a> {
    pub account: &'a Account,
    pub unread: u32,
    pub active: bool,
}

/// Script that redraws the rail.
pub fn rail_script(items: &[RailItem], locked: bool, manage: bool, bar_with_one: bool) -> String {
    // While locked the rail draws only the logo, so don't even send the names.
    let shown = if locked { &[][..] } else { items };
    let accounts: Vec<_> = shown
        .iter()
        .map(|i| {
            serde_json::json!({
                "id": i.account.id,
                "name": i.account.name,
                "initials": i.account.initials(),
                "color": i.account.color,
                "unread": i.unread,
                "active": i.active,
            })
        })
        .collect();
    let state = serde_json::json!({
        "accounts": accounts,
        "locked": locked,
        "manage": manage && !locked,
        "canAdd": items.len() < MAX_ACCOUNTS,
        "barWithOne": bar_with_one,
    });
    format!("window.__rail && window.__rail.render({state})")
}

/// Move an account one place up or down; returns whether anything moved.
pub fn move_account(accounts: &mut [Account], id: &str, up: bool) -> bool {
    let Some(i) = accounts.iter().position(|a| a.id == id) else {
        return false;
    };
    let j = if up {
        i.checked_sub(1)
    } else {
        (i + 1 < accounts.len()).then_some(i + 1)
    };
    match j {
        Some(j) => {
            accounts.swap(i, j);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_keeps_the_original_session() {
        let (mut list, mut active) = (Vec::new(), String::new());
        normalize(&mut list, &mut active);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, DEFAULT_ID);
        assert_eq!(active, DEFAULT_ID);
        let base = Path::new("/data/webview");
        assert_eq!(data_dir(base, DEFAULT_ID), base);
        assert_eq!(data_dir(base, "a1b2"), Path::new("/data/webview-a1b2"));
    }

    #[test]
    fn bad_saved_accounts_are_dropped() {
        let acct = |id: &str| Account {
            id: id.into(),
            name: " Work \n".into(),
            color: "#000".into(),
        };
        let mut list = vec![acct("../evil"), acct("ab12"), acct("ab12"), acct("")];
        let mut active = "gone".to_string();
        normalize(&mut list, &mut active);
        assert_eq!(list.len(), 1, "path-like, duplicate and empty ids are removed");
        assert_eq!(list[0].name, "Work");
        assert_eq!(active, "ab12");
    }

    #[test]
    fn new_accounts_get_unique_names_and_colors() {
        let mut list = Vec::new();
        let mut active = String::new();
        normalize(&mut list, &mut active);
        for _ in 0..3 {
            let a = new_account(&list);
            assert!(valid_id(&a.id));
            list.push(a);
        }
        let names: Vec<_> = list.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Personal", "Account 1", "Account 2", "Account 3"]);
        let colors: std::collections::HashSet<_> = list.iter().map(|a| a.color.clone()).collect();
        assert_eq!(colors.len(), 4);
    }

    #[test]
    fn names_and_initials() {
        assert_eq!(clean_name("  My   Work  "), Some("My Work".into()));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"x".repeat(50)).unwrap().len(), 32);
        assert_eq!(initials("Personal"), "PE");
        assert_eq!(initials("my work"), "MW");
        assert_eq!(initials("Ünïcode name"), "ÜN");
        assert_eq!(initials("!!!"), "?");
    }

    #[test]
    fn layout_and_when_the_bar_shows() {
        assert_eq!(layout(false, 1000.0), (0.0, 0.0, 1000.0));
        assert_eq!(
            layout(true, 1000.0),
            (RAIL_WIDTH, RAIL_WIDTH, 1000.0 - RAIL_WIDTH)
        );
        assert!(show_bar(1, true));
        assert!(!show_bar(1, false), "one account, bar turned off");
        assert!(show_bar(2, false), "two accounts always need it");
    }

    #[test]
    fn orphans_are_only_removed_account_folders() {
        let root = std::env::temp_dir().join(format!("wisp-acct-{}", std::process::id()));
        let base = root.join("webview");
        for d in [
            "webview",
            "webview-aa11",
            "webview-bb22",
            "webview-not hex",
            "other",
        ] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        let keep = vec![Account {
            id: "aa11".into(),
            name: "A".into(),
            color: "#000".into(),
        }];
        let orphans = orphaned_dirs(&base, &keep);
        assert_eq!(orphans, vec![root.join("webview-bb22")]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn rail_messages_and_moves() {
        assert_eq!(
            RailMsg::parse(r#"{"kind":"select","id":"ab"}"#),
            Some(RailMsg::Select { id: "ab".into() })
        );
        assert_eq!(
            RailMsg::parse(r#"{"kind":"manage","open":true}"#),
            Some(RailMsg::Manage { open: true })
        );
        assert_eq!(RailMsg::parse(r#"{"kind":"nope"}"#), None);
        assert_eq!(
            RailMsg::parse(r#"{"kind":"bar","with_one":false}"#),
            Some(RailMsg::Bar { with_one: false })
        );
        let acct = |id: &str| Account {
            id: id.into(),
            name: id.into(),
            color: "#000".into(),
        };
        let mut list = vec![acct("a"), acct("b"), acct("c")];
        assert!(move_account(&mut list, "c", true));
        assert!(!move_account(&mut list, "a", true), "first can't move up");
        assert!(!move_account(&mut list, "b", false), "last can't move down");
        let ids: Vec<_> = list.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["a", "c", "b"]);
    }

    #[test]
    fn locked_rail_never_lists_accounts_in_manage_mode() {
        let a = Account {
            id: "a".into(),
            name: "Secret".into(),
            color: "#000".into(),
        };
        let js = rail_script(
            &[RailItem {
                account: &a,
                unread: 3,
                active: true,
            }],
            true,
            true,
            true,
        );
        assert!(js.contains(r#""manage":false"#));
        assert!(js.contains(r#""locked":true"#));
        assert!(!js.contains("Secret"), "names aren't sent while locked");
    }

    #[test]
    fn notification_titles() {
        assert_eq!(notification_title("Work", "Ann", true), "Work · Ann");
        assert_eq!(notification_title("Work", "Ann", false), "Ann");
    }
}
