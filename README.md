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
Chromium, so the download is one or two megabytes and the app starts almost instantly. On top of the web
client it adds a native window, a tray icon with an unread badge, desktop notifications and an
optional cleaner theme.

Wisp talks to WhatsApp only through the official web client, exactly like a browser tab. It doesn't
reimplement the protocol, automate your account or read your messages.

> [!NOTE]
> Wisp is an independent open-source project. It is not affiliated with, endorsed by or connected
> to WhatsApp or Meta. "WhatsApp" is a trademark of its owner.

## Features

- **Small and fast.** About 1 MB to download (2 MB for the universal macOS app). The window shows
  in a fraction of a second.
- **Modern theme.** System fonts, slim scrollbars, rounded chat rows, pill-shaped search and
  composer, softer message bubbles and an accent color you choose. Toggle it any time with
  <kbd>Ctrl/Cmd</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd>.
- **Privacy blur.** Blurs chat names, message previews, avatars and messages until you hover over
  them, and hides who wrote and what they wrote from notifications. Handy in public, at work or
  when sharing your screen. Toggle it with <kbd>Ctrl/Cmd</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd>.
- **Multiple accounts.** Use up to 9 WhatsApp accounts side by side, each with its own session and
  unread badge. Switch with one click or <kbd>Ctrl/Cmd</kbd>+<kbd>1</kbd>…<kbd>9</kbd>; accounts in
  the background keep receiving messages and notifications.
- **App lock.** Keeps your chats behind a lock screen when Wisp starts and after you've been away
  for a while. Unlock with Windows Hello, Touch ID (or your Mac password), or a Wisp PIN. Messages
  keep arriving while it's locked, and notifications don't show who wrote or what.
- **Your own CSS.** Drop a `user.css` next to the settings file and it's applied on every start.
- **Tray icon** with an unread badge. Closing the window keeps Wisp running in the tray.
- **Native notifications** for new messages, under Wisp's own name and icon, with an option to hide
  message previews. On Windows, clicking one opens Wisp on the account the message is for, and Wisp
  adds a Start menu entry the first time it runs if there isn't one, because Windows only shows an
  app's notifications once it has one.
- **Easy on resources in the background.** When the window is minimized or in the tray, the page is
  throttled like a background browser tab and the webview trims its memory. Messages and
  notifications keep arriving.
- **Stays signed in.** Your session lives in Wisp's data folder.
- **Safe links.** Links open in your default browser. Wisp itself only ever navigates to
  `web.whatsapp.com`.
- **Downloads** go to your Downloads folder and never overwrite existing files.
- **Remembers** window size, position, zoom and the account you had open.

### How it compares

Compared on Windows 11 with the Microsoft Store WhatsApp app (which also runs WhatsApp Web in
WebView2), with the same account, using [`bench/bench.ps1`](bench/bench.ps1). Sizes are for Wisp
0.2.0; timing, CPU and memory were measured with Wisp 0.1.0 on one machine, so treat them as
indicative:

| | Wisp | Store app |
|---|---|---|
| Download (Windows zip) | **0.8 MB** | — |
| Installed size | **1.5 MB** | 331 MB |
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

Unzip and double-click **Install Wisp.cmd**. It installs Wisp for your user account (no
administrator rights needed) in `%LOCALAPPDATA%\Programs\Wisp`, adds it to the Start menu and the
desktop, and starts it. Run it again to update; your settings and sessions are kept.

To remove Wisp, use **Settings → Apps → Wisp → Uninstall**. Your settings and WhatsApp sessions
stay on your computer unless you run `install.ps1 -Uninstall -Purge` from the install folder.

Wisp isn't code-signed yet, so the first time you run it Windows may show **Windows protected
your PC**. Choose **More info → Run anyway**.

You can also skip installing and run `Wisp.exe` straight from the folder. Wisp needs the
Microsoft Edge WebView2 Runtime, which ships with Windows 10 and 11.
</details>

<details>
<summary><b>macOS</b></summary>

Unzip and move `Wisp.app` to Applications. Wisp works on macOS 11 or later; using more than one
account needs macOS 14 or later.

The build is ad-hoc signed, not notarized, so macOS blocks it the first time. Open **System
Settings → Privacy & Security**, scroll down and click **Open Anyway** (on macOS 14 and earlier you
can also right-click the app and choose **Open**). Or clear the download flag in Terminal:

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

On first start, scan the QR code with WhatsApp on your phone, just like WhatsApp Web: open
**Linked devices** (under **Settings** on iPhone, or the **⋮** menu on Android) and tap **Link a
device**.

| Shortcut | Action |
|---|---|
| <kbd>Ctrl/Cmd</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> | Toggle the modern theme |
| <kbd>Ctrl/Cmd</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd> | Toggle privacy blur |
| <kbd>Ctrl/Cmd</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd> | Lock Wisp (when app lock is on) |
| <kbd>Ctrl/Cmd</kbd>+<kbd>=</kbd> / <kbd>-</kbd> / <kbd>0</kbd> | Zoom in / out / reset |
| <kbd>Ctrl/Cmd</kbd>+<kbd>1</kbd>…<kbd>9</kbd> | Switch to account 1–9 |
| <kbd>Ctrl</kbd>+<kbd>Tab</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Tab</kbd> | Next / previous account |
| <kbd>Ctrl/Cmd</kbd>+<kbd>R</kbd> or <kbd>F5</kbd> | Reload |
| <kbd>Ctrl/Cmd</kbd>+<kbd>W</kbd> | Hide to the tray |
| <kbd>Ctrl/Cmd</kbd>+<kbd>Q</kbd> | Quit |

The tray menu also lets you add an account, toggle the theme, privacy blur, app lock and
notifications, lock Wisp, reload, or quit.

### Multiple accounts

The slim account bar on the left shows each account with its initials, color and unread count.
Click **+** there (or **Add account** in the tray menu) and scan the QR code with the other phone.
Click an account to switch to it. Right-click it, or click **⚙**, to rename, reorder or remove
accounts. Removing an account signs it out of Wisp and deletes its session from this computer; the
phone may keep listing Wisp under **Linked devices** for a while, and you can remove it there too.

With a single account you can hide the bar: untick **Show the account bar when there's only one
account** under **⚙**. It always shows once you have two.

Each account runs in its own webview with its own data folder, so sessions never mix. Accounts you
aren't looking at rest in the background like a minimized window. Theme, privacy blur, app lock
and zoom apply to all accounts. macOS only keeps web sessions apart from macOS 14 on, so on older
versions Wisp offers a single account.

### App lock

Turn it on from the tray menu (**App lock**) and choose a PIN. Where your computer supports it, you
unlock with **Windows Hello** or **Touch ID / your Mac password**, and the PIN is a backup for when
that isn't available. On Linux you unlock with the PIN.

Wisp locks when it starts, when you press <kbd>Ctrl/Cmd</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd> or
choose **Lock now**, and after the window has been unfocused, minimized or in the tray for
`lock_after_minutes` (5 by default; 0 turns that off). After 5 wrong PINs, PIN entry pauses for 30
seconds. Only a salted, slow hash of the PIN is stored. To change the PIN, turn app lock off and
on again.

App lock keeps people who use your computer from reading your chats in Wisp. It isn't a substitute
for locking your computer: someone signed in as you could still read Wisp's files.

## Configuration

Settings live in `config.toml`, created on first run. WhatsApp sessions are kept separately:

| OS | Settings (`config.toml`, `user.css`) | WhatsApp sessions |
|---|---|---|
| Windows | `%APPDATA%\Wisp\config\` | `%LOCALAPPDATA%\Wisp\data\` |
| macOS | `~/Library/Application Support/Wisp/` | WebKit's data store for Wisp (in `~/Library`) |
| Linux | `~/.config/wisp/` | `~/.local/share/wisp/` |

```toml
theme_enabled = true
privacy_blur = false        # blur chats until hovered; hide notification contents
app_lock = false            # turn on from the tray, which also asks for a PIN
lock_after_minutes = 5      # lock after this long away; 0 = only on start or by hand
account_bar = true          # show the account bar even with a single account
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
to WhatsApp's servers. Your sessions and settings stay on your computer, in the folders above.

## Building from source

You need [Rust](https://rustup.rs) 1.90 or newer.

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
