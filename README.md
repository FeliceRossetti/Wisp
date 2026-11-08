<p align="center">
  <img src="assets/logo.png" alt="Wisp logo" width="128" height="128">
</p>

<h1 align="center">Wisp</h1>

<p align="center">
  <b>A lightweight, good-looking desktop app for WhatsApp, written in Rust.</b><br>
  Windows · macOS · Linux
</p>

<p align="center">
  <a href="https://github.com/FeliceRossetti/Wisp/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/FeliceRossetti/Wisp/actions/workflows/ci.yml/badge.svg?branch=dev"></a>
  <a href="https://github.com/FeliceRossetti/Wisp/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/FeliceRossetti/Wisp?sort=semver"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue"></a>
  <img alt="Written in Rust" src="https://img.shields.io/badge/made%20with-Rust-b7410e">
</p>

---

Wisp wraps the official [WhatsApp Web](https://web.whatsapp.com) in your operating system's own
webview: WebView2 on Windows, WKWebView on macOS and WebKitGTK on Linux. It doesn't ship a copy of
Chromium, so the download is about a megabyte and the app starts almost instantly. On top of the web
client it adds a native window, a tray icon with an unread badge, desktop notifications and an
optional cleaner theme.

Wisp talks to WhatsApp only through the official web client, exactly like a browser tab. It doesn't
reimplement the protocol, automate your account or read your messages.

> [!NOTE]
> Wisp is an independent open-source project. It is not affiliated with, endorsed by or connected
> to WhatsApp or Meta. "WhatsApp" is a trademark of its owner.

## Features

- **Small and fast.** About 1 MB to download. The window shows in a fraction of a second.
- **Modern theme.** System fonts, slim scrollbars, rounded chat rows, pill-shaped search and
  composer, softer message bubbles and an accent color you choose. Toggle it any time with
  <kbd>Ctrl/Cmd</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd>.
- **Your own CSS.** Drop a `user.css` next to the settings file and it's applied on every start.
- **Tray icon** with an unread badge. Closing the window keeps Wisp running in the tray.
- **Native notifications** for new messages, with an option to hide message previews.
- **Easy on resources in the background.** When the window is minimized or in the tray, the page is
  throttled like a background browser tab and the webview trims its memory. Messages and
  notifications keep arriving.
- **Stays signed in.** Your session lives in Wisp's data folder.
- **Safe links.** Links open in your default browser. Wisp itself only ever navigates to
  `web.whatsapp.com`.
- **Downloads** go to your Downloads folder and never overwrite existing files.
- **Remembers** window size, position and zoom.

### How it compares

Measured on Windows 11 against the Microsoft Store WhatsApp app (which also runs WhatsApp Web in
WebView2), with the same account, using [`bench/bench.ps1`](bench/bench.ps1). One machine, so treat
these as indicative:

| | Wisp | Store app |
|---|---|---|
| Download size | **1.1 MB** | 331 MB |
| Window visible after launch | **0.3 s** | 1.7 s |
| CPU time in the first 45 s | **13.8 s** | 17.0 s |
| RAM in use, window in background | **~0.2–0.5 GB** | ~0.9 GB |
| Idle CPU | < 1% of one core | < 2% of one core |

The Store app commits somewhat less memory overall (about 0.2 GB), because part of its work happens in
its native host process. Run the benchmark on your own machine to compare.

## Install

Download the latest build for your platform from the
[Releases page](https://github.com/FeliceRossetti/Wisp/releases/latest). Each release lists SHA-256 checksums.

<details>
<summary><b>Windows</b></summary>

Unzip and run `Wisp.exe`. Wisp needs the Microsoft Edge WebView2 Runtime, which ships with
Windows 10 and 11.
</details>

<details>
<summary><b>macOS</b></summary>

Unzip and move `Wisp.app` to Applications. The build is ad-hoc signed, not notarized, so the first
time either right-click the app and choose **Open**, or run:

```bash
xattr -dr com.apple.quarantine /Applications/Wisp.app
```
</details>

<details>
<summary><b>Linux</b></summary>

Wisp needs WebKitGTK 4.1 and, for the tray icon, libayatana-appindicator. On Debian or Ubuntu:

```bash
sudo apt install libwebkit2gtk-4.1-0 libayatana-appindicator3-1
```

Extract the tarball and run the installer. It installs for your user only and adds Wisp to your app
launcher.

```bash
./install.sh
```

Remove it again with `./install.sh --uninstall`.
</details>

## Using Wisp

On first start, scan the QR code with WhatsApp on your phone (**Settings → Linked devices**), just
like WhatsApp Web.

| Shortcut | Action |
|---|---|
| <kbd>Ctrl/Cmd</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> | Toggle the modern theme |
| <kbd>Ctrl/Cmd</kbd>+<kbd>=</kbd> / <kbd>-</kbd> / <kbd>0</kbd> | Zoom in / out / reset |
| <kbd>Ctrl/Cmd</kbd>+<kbd>R</kbd> or <kbd>F5</kbd> | Reload |
| <kbd>Ctrl/Cmd</kbd>+<kbd>W</kbd> | Hide to the tray |
| <kbd>Ctrl/Cmd</kbd>+<kbd>Q</kbd> | Quit |

The tray menu also lets you toggle the theme and notifications, reload, or quit.

## Configuration

Settings live in `config.toml`, created on first run:

| OS | Folder |
|---|---|
| Windows | `%APPDATA%\Wisp\config\` |
| macOS | `~/Library/Application Support/Wisp/` |
| Linux | `~/.config/wisp/` |

```toml
theme_enabled = true
accent = "#06bcbc"          # any CSS color
close_to_tray = true
start_hidden = false
notifications = true
notification_preview = true # false = sender name only
zoom = 1.0
user_agent = ""             # empty = a sensible default per platform
```

### Custom CSS

Create `user.css` in the same folder. It's loaded after the built-in theme, whether the theme is on
or off. The theme sets the `--wisp-accent` variable, and rules scoped under `html.wisp-themed` only
apply while the theme is on:

```css
html.wisp-themed {
  --wisp-radius: 18px;
}
```

## Privacy

Wisp has no telemetry, analytics or update checks. The only network traffic is WhatsApp Web's own,
to WhatsApp's servers. Your session and settings stay on your computer in the folders above.

## Building from source

You need [Rust](https://rustup.rs) 1.85 or newer.

<details>
<summary>Linux system libraries</summary>

```bash
sudo apt install pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libdbus-1-dev libayatana-appindicator3-dev
```
</details>

```bash
git clone https://github.com/FeliceRossetti/Wisp.git
cd Wisp
cargo run              # debug build, with webview devtools
cargo build --release  # optimized build in target/release/
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow.

## Benchmarking (Windows)

[`bench/bench.ps1`](bench/bench.ps1) compares Wisp with the Microsoft Store WhatsApp. It measures
whole process trees: the host process plus every WebView2 process. The `ColdStart` and `Background`
modes close and reopen both apps.

```powershell
powershell -ExecutionPolicy Bypass -File bench\bench.ps1 -Mode ColdStart -Runs 3 -Only "Wisp,WhatsApp (Store)"
powershell -ExecutionPolicy Bypass -File bench\bench.ps1 -Mode Background
powershell -ExecutionPolicy Bypass -File bench\bench.ps1 -Mode Snapshot
```

## Contributing

Bug reports, ideas and pull requests are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and
our [Code of Conduct](CODE_OF_CONDUCT.md) first. To report a security issue, see
[SECURITY.md](SECURITY.md).

## Acknowledgements

Wisp stands on [wry](https://github.com/tauri-apps/wry), [tao](https://github.com/tauri-apps/tao)
and [tray-icon](https://github.com/tauri-apps/tray-icon) from the Tauri project.

## License

[MIT](LICENSE)
