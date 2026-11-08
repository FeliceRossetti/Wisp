# Changelog

All notable changes to Wisp are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Wisp follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Privacy blur: chat names, previews, avatars and messages stay blurred until hovered, and
  notifications only say "New message". Toggle with Ctrl/Cmd+Shift+B or from the tray.

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

[Unreleased]: https://github.com/FeliceRossetti/Wisp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/FeliceRossetti/Wisp/releases/tag/v0.1.0
