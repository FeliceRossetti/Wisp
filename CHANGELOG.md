# Changelog

All notable changes to Wisp are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Wisp follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-11-13

### Added

- Settings: one screen for accounts, the theme, accent color and zoom, privacy blur and
  notifications, app lock (including changing the PIN and when it locks automatically) and startup.
  Open it with **⚙** in the account bar, **Settings** in the tray menu or Ctrl/Cmd+,. It replaces the
  manage-accounts screen.
- Open Wisp when you sign in: an option in Settings that starts Wisp quietly in the tray at sign-in
  on Windows, macOS and Linux. Uninstalling Wisp turns it off again.
- Removing an account is press-and-hold: hold **Hold to remove** for two seconds while Settings
  shakes harder and harder; let go early and nothing happens. Settings stays open afterwards.
- Pick the accent color: six presets, from the logo's teal to pure graphite, or any color.

### Changed

- The theme is now calm and monochrome: WhatsApp's greens and blues become graphite (or soft white
  in light mode) with a single accent, the logo's teal, for unread badges, read ticks, the active
  account and focus. The chat background is flat, without the wallpaper's gradient and doodles.
  Rows, bubbles, menus and inputs get softer shapes and quick, smooth transitions.
- Smoother everywhere: Settings fades in over your chat and back out, switching accounts fades the
  next one in, the lock screen fades away when you unlock and shakes at a wrong PIN, and turning the
  theme or privacy blur on or off, or picking an accent, crossfades instead of snapping. Switches,
  the active account ring and unread badges animate too. Reduced-motion settings are respected.
- The account bar, Settings and lock screen use the same graphite palette, follow WhatsApp's own
  dark or light setting, and show every account with the same neutral avatar.

### Fixed

- A newly added account stayed blank until you switched to another account and back.
- After adding an account, opening Settings hid the account bar and showed nothing you could use.

## [0.2.0] - 2026-11-10

### Added

- Multiple accounts: up to 9 WhatsApp accounts side by side, each with its own session, color and
  unread badge. Switch with the account bar on the left, Ctrl/Cmd+1…9 or Ctrl+Tab; add, rename,
  reorder or remove accounts from it. Notifications say which account they're for. Your existing
  session becomes the first account. On macOS, more than one account needs macOS 14 or later.
- App lock: hides your chats behind a lock screen on start, on demand (Ctrl/Cmd+Shift+L or the
  tray) and after a while away. Unlock with Windows Hello, Touch ID / your Mac password, or a PIN
  (stored only as a salted hash; 5 wrong tries pause entry for 30 seconds). Messages keep arriving
  while locked, and notifications stay neutral.
- Privacy blur: chat names, previews, avatars and messages stay blurred until hovered, and
  notifications only say "New message". Toggle with Ctrl/Cmd+Shift+B or from the tray.
- Windows installer: "Install Wisp.cmd" in the Windows download installs Wisp for your user
  account with Start menu and desktop shortcuts, and an uninstall entry in Settings → Apps.

### Fixed

- Windows notifications come from "Wisp" with its icon instead of "Windows PowerShell", and
  clicking one opens Wisp on the account the message is for.
- Theme and privacy blur keep their current setting when a page reloads.

### Changed

- Building from source needs Rust 1.90 or later.

## [0.1.0] - 2026-11-08

First public release.

### Added

- Desktop app for WhatsApp Web on Windows, macOS and Linux, using the system webview.
- Optional modern theme with a configurable accent color, toggled with Ctrl/Cmd+Shift+T or from the
  tray, plus support for a custom `user.css`.
- Tray icon with an unread badge, and close-to-tray.
- Native desktop notifications, with an option to hide message previews.
- Background resting: the page is throttled and memory trimmed while the window is hidden, minimized
  or left unfocused.
- External links open in the default browser; downloads go to the Downloads folder without
  overwriting files.
- Window size, position and zoom are remembered.
- Windows benchmark script comparing Wisp with the Microsoft Store app.
- Release packages: Windows zip, macOS universal app, and Linux tarball with a user installer.

[Unreleased]: https://github.com/FeliceRossetti/Wisp/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/FeliceRossetti/Wisp/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/FeliceRossetti/Wisp/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/FeliceRossetti/Wisp/releases/tag/v0.1.0
