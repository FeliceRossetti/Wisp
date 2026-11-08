//! App lock: WhatsApp keeps running (messages still arrive) but the page stays
//! hidden behind a lock screen until the user proves it's them.
//!
//! Unlocking uses the operating system where it can (Windows Hello, Touch ID or
//! the macOS account password) and a Wisp PIN everywhere else. Only a salted,
//! slow hash of the PIN is ever stored.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Wrong PINs allowed before a pause.
const MAX_ATTEMPTS: u32 = 5;
/// How long PIN entry is blocked after too many wrong tries.
const LOCKOUT: Duration = Duration::from_secs(30);
/// PBKDF2-HMAC-SHA256 rounds for the PIN hash.
const ROUNDS: u32 = 210_000;

/// A stored PIN: salt and PBKDF2 hash, both hex-encoded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinHash {
    pub salt: String,
    pub hash: String,
}

impl PinHash {
    pub fn new(pin: &str) -> Self {
        let mut salt = [0u8; 16];
        getrandom::fill(&mut salt).expect("the system random number generator works");
        Self {
            salt: hex(&salt),
            hash: hex(&derive(pin, &salt)),
        }
    }

    pub fn verify(&self, pin: &str) -> bool {
        let Some(salt) = unhex(&self.salt) else {
            return false;
        };
        let Some(expected) = unhex(&self.hash) else {
            return false;
        };
        constant_time_eq(&derive(pin, &salt), &expected)
    }
}

/// PINs are 4 to 12 digits.
pub fn valid_pin(pin: &str) -> bool {
    (4..=12).contains(&pin.len()) && pin.bytes().all(|b| b.is_ascii_digit())
}

fn derive(pin: &str, salt: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(pin.as_bytes(), salt, ROUNDS, &mut out);
    out
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Counts wrong PINs and pauses entry after too many.
#[derive(Default)]
pub struct Attempts {
    failed: u32,
    blocked_until: Option<Instant>,
}

impl Attempts {
    /// Seconds left before another PIN may be tried, if blocked.
    pub fn blocked_for(&self, now: Instant) -> Option<u64> {
        self.blocked_until
            .filter(|&t| t > now)
            .map(|t| (t - now).as_secs() + 1)
    }

    pub fn record_failure(&mut self, now: Instant) {
        self.failed += 1;
        if self.failed >= MAX_ATTEMPTS {
            self.failed = 0;
            self.blocked_until = Some(now + LOCKOUT);
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The lock as the app sees it. The page only draws this state.
pub struct LockState {
    /// The page is hidden behind the lock screen.
    pub locked: bool,
    /// The "choose a PIN" screen is open (turning app lock on).
    pub setup: bool,
    /// Waiting for the system prompt (Windows Hello, Touch ID).
    busy: bool,
    message: String,
    attempts: Attempts,
    /// When the window stopped being in use (unfocused, hidden or minimized).
    away_since: Option<Instant>,
    system: Option<&'static str>,
}

impl LockState {
    pub fn new(start_locked: bool, system: Option<&'static str>) -> Self {
        Self {
            locked: start_locked,
            setup: false,
            busy: false,
            message: String::new(),
            attempts: Attempts::default(),
            away_since: None,
            system,
        }
    }

    /// Script that shows this state in the page.
    pub fn script(&self) -> String {
        crate::web::lock_script(
            self.locked,
            self.setup,
            self.busy,
            &self.message,
            self.system.unwrap_or(""),
        )
    }

    pub fn lock(&mut self) {
        self.locked = true;
        self.setup = false;
        self.busy = false;
        self.message.clear();
    }

    fn unlock(&mut self) {
        self.locked = false;
        self.busy = false;
        self.message.clear();
        self.attempts.reset();
        self.away_since = None;
    }

    pub fn begin_setup(&mut self) {
        self.setup = true;
        self.message.clear();
    }

    pub fn end_setup(&mut self) {
        self.setup = false;
        self.message.clear();
    }

    /// Whether a system prompt should be started now.
    pub fn start_system_auth(&mut self) -> bool {
        if !self.locked || self.busy || self.system.is_none() {
            return false;
        }
        self.busy = true;
        self.message.clear();
        true
    }

    pub fn finish_system_auth(&mut self, result: SystemAuth) {
        self.busy = false;
        match result {
            SystemAuth::Verified => self.unlock(),
            SystemAuth::Canceled => self.message.clear(),
            SystemAuth::Failed => self.message = "That didn't work. Try again or use your PIN.".into(),
            SystemAuth::Unavailable => {
                self.message = format!(
                    "{} isn't available right now. Use your PIN.",
                    self.system.unwrap_or("It")
                )
            }
        }
    }

    /// Check a PIN typed on the lock screen.
    pub fn try_pin(&mut self, stored: Option<&PinHash>, pin: &str, now: Instant) {
        if !self.locked {
            return;
        }
        if let Some(secs) = self.attempts.blocked_for(now) {
            self.message = format!("Too many tries. Wait {secs} seconds.");
            return;
        }
        if stored.is_some_and(|p| p.verify(pin)) {
            self.unlock();
        } else {
            self.attempts.record_failure(now);
            self.message = match self.attempts.blocked_for(now) {
                Some(secs) => format!("Too many tries. Wait {secs} seconds."),
                None => "Wrong PIN.".into(),
            };
        }
    }

    /// Track whether the window is in use, for locking after a while away.
    pub fn set_away(&mut self, away: bool, now: Instant) {
        match (away, self.away_since) {
            (true, None) => self.away_since = Some(now),
            (false, Some(_)) => self.away_since = None,
            _ => {}
        }
    }

    /// Lock once the window has been away long enough. Returns whether the
    /// state changed and when to check again.
    pub fn check_idle(&mut self, enabled: bool, after_minutes: u32, now: Instant) -> (bool, Option<Instant>) {
        let (Some(since), true, false, true) = (self.away_since, enabled, self.locked, after_minutes > 0)
        else {
            return (false, None);
        };
        let due = since + Duration::from_secs(u64::from(after_minutes) * 60);
        if now >= due {
            self.lock();
            (true, None)
        } else {
            (false, Some(due))
        }
    }
}

/// Outcome of asking the operating system to verify the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAuth {
    Verified,
    Failed,
    Canceled,
    Unavailable,
}

/// The name of the system unlock method, if this computer has one set up.
pub fn system_method() -> Option<&'static str> {
    platform::available().then_some(platform::LABEL)
}

/// Ask the operating system to verify the user. `done` runs on another thread.
pub fn request_system_auth(window: &tao::window::Window, done: impl FnOnce(SystemAuth) + Send + 'static) {
    platform::request(window, "Unlock Wisp", done);
}

#[cfg(windows)]
mod platform {
    use super::SystemAuth;
    use tao::platform::windows::WindowExtWindows;
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    use windows::core::{HSTRING, factory};
    use windows_future::IAsyncOperation;

    pub const LABEL: &str = "Windows Hello";

    pub fn available() -> bool {
        UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.join())
            .is_ok_and(|a| a == UserConsentVerifierAvailability::Available)
    }

    pub fn request(
        window: &tao::window::Window,
        reason: &str,
        done: impl FnOnce(SystemAuth) + Send + 'static,
    ) {
        // The "for window" variant parents the Hello prompt to Wisp, so it opens in front.
        let op = factory::<UserConsentVerifier, IUserConsentVerifierInterop>().and_then(|interop| unsafe {
            interop.RequestVerificationForWindowAsync::<IAsyncOperation<UserConsentVerificationResult>>(
                HWND(window.hwnd() as _),
                &HSTRING::from(reason),
            )
        });
        let Ok(op) = op else {
            done(SystemAuth::Unavailable);
            return;
        };
        std::thread::spawn(move || {
            done(match op.join() {
                Ok(UserConsentVerificationResult::Verified) => SystemAuth::Verified,
                Ok(UserConsentVerificationResult::Canceled) => SystemAuth::Canceled,
                Ok(
                    UserConsentVerificationResult::DeviceNotPresent
                    | UserConsentVerificationResult::NotConfiguredForUser
                    | UserConsentVerificationResult::DisabledByPolicy,
                ) => SystemAuth::Unavailable,
                _ => SystemAuth::Failed,
            })
        });
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::SystemAuth;
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    use std::sync::Mutex;

    pub const LABEL: &str = "Touch ID or your password";

    pub fn available() -> bool {
        let ctx = unsafe { LAContext::new() };
        unsafe { ctx.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication) }.is_ok()
    }

    pub fn request(
        _window: &tao::window::Window,
        reason: &str,
        done: impl FnOnce(SystemAuth) + Send + 'static,
    ) {
        let ctx = unsafe { LAContext::new() };
        let done = Mutex::new(Some(done));
        // macOS cancels the request if the context is freed, so the reply keeps it alive.
        let keep_alive = ctx.clone();
        let reply = RcBlock::new(move |ok: Bool, _err: *mut NSError| {
            let _ = &keep_alive;
            if let Some(done) = done.lock().ok().and_then(|mut d| d.take()) {
                done(if ok.as_bool() {
                    SystemAuth::Verified
                } else {
                    SystemAuth::Failed
                });
            }
        });
        unsafe {
            ctx.evaluatePolicy_localizedReason_reply(
                LAPolicy::DeviceOwnerAuthentication,
                &NSString::from_str(reason),
                &reply,
            );
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use super::SystemAuth;

    pub const LABEL: &str = "";

    pub fn available() -> bool {
        false
    }

    pub fn request(
        _window: &tao::window::Window,
        _reason: &str,
        done: impl FnOnce(SystemAuth) + Send + 'static,
    ) {
        done(SystemAuth::Unavailable);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_round_trip() {
        let p = PinHash::new("482916");
        assert!(p.verify("482916"));
        assert!(!p.verify("482917"));
        assert!(!p.verify(""));
        assert_ne!(PinHash::new("482916").salt, p.salt, "every PIN gets a fresh salt");
        assert!(!p.hash.contains("482916"));
    }

    #[test]
    fn corrupted_hash_never_verifies() {
        let bad = PinHash {
            salt: "zz".into(),
            hash: "00".into(),
        };
        assert!(!bad.verify("1234"));
    }

    #[test]
    fn pin_rules() {
        assert!(valid_pin("1234"));
        assert!(valid_pin("123456789012"));
        assert!(!valid_pin("123"));
        assert!(!valid_pin("1234567890123"));
        assert!(!valid_pin("12a4"));
        assert!(!valid_pin("１２３４"), "only ASCII digits");
    }

    #[test]
    fn pin_unlocks_and_wrong_pins_pause() {
        let pin = PinHash::new("2468");
        let t0 = Instant::now();
        let mut s = LockState::new(true, None);
        for _ in 0..5 {
            s.try_pin(Some(&pin), "0000", t0);
        }
        assert!(s.locked);
        assert!(s.script().contains("Too many tries"));
        s.try_pin(Some(&pin), "2468", t0);
        assert!(s.locked, "even the right PIN waits out the pause");
        s.try_pin(Some(&pin), "2468", t0 + LOCKOUT);
        assert!(!s.locked);
    }

    #[test]
    fn no_stored_pin_never_unlocks() {
        let mut s = LockState::new(true, None);
        s.try_pin(None, "", Instant::now());
        assert!(s.locked);
    }

    #[test]
    fn system_auth_only_while_locked() {
        let mut s = LockState::new(false, Some("Windows Hello"));
        assert!(!s.start_system_auth());
        s.lock();
        assert!(s.start_system_auth());
        assert!(!s.start_system_auth(), "one prompt at a time");
        s.finish_system_auth(SystemAuth::Canceled);
        assert!(s.locked);
        assert!(s.start_system_auth());
        s.finish_system_auth(SystemAuth::Verified);
        assert!(!s.locked);
    }

    #[test]
    fn locks_after_time_away() {
        let t0 = Instant::now();
        let mut s = LockState::new(false, None);
        s.set_away(true, t0);
        assert_eq!(
            s.check_idle(true, 5, t0 + Duration::from_secs(60)),
            (false, Some(t0 + Duration::from_secs(300)))
        );
        assert_eq!(
            s.check_idle(false, 5, t0 + Duration::from_secs(600)),
            (false, None),
            "app lock off"
        );
        assert_eq!(
            s.check_idle(true, 0, t0 + Duration::from_secs(600)),
            (false, None),
            "0 = never"
        );
        assert_eq!(s.check_idle(true, 5, t0 + Duration::from_secs(300)), (true, None));
        assert!(s.locked);
        s.set_away(false, t0);
        let mut t = LockState::new(false, None);
        t.set_away(true, t0);
        t.set_away(false, t0 + Duration::from_secs(200));
        assert_eq!(
            t.check_idle(true, 5, t0 + Duration::from_secs(400)),
            (false, None),
            "coming back resets it"
        );
    }

    #[test]
    fn attempts_pause_after_five_failures() {
        let mut a = Attempts::default();
        let t0 = Instant::now();
        for _ in 0..4 {
            a.record_failure(t0);
        }
        assert_eq!(a.blocked_for(t0), None);
        a.record_failure(t0);
        assert!(a.blocked_for(t0).is_some());
        assert_eq!(a.blocked_for(t0 + LOCKOUT), None);
        a.reset();
        assert_eq!(a.blocked_for(t0), None);
    }
}
