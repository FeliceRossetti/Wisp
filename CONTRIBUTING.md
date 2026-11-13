# Contributing to Wisp

Thanks for your interest in Wisp! Bug reports, ideas, docs fixes and code are all welcome. This guide
explains how the project works so your contribution lands smoothly.

By taking part you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Contents

- [Before you start](#before-you-start)
- [Reporting bugs](#reporting-bugs)
- [Suggesting features](#suggesting-features)
- [Development setup](#development-setup)
- [Project layout](#project-layout)
- [Making changes](#making-changes)
- [Branches and pull requests](#branches-and-pull-requests)
- [Releases](#releases)

## Before you start

Wisp has a deliberately narrow scope. Keep these principles in mind:

- **Official web client only.** Wisp loads `web.whatsapp.com` and nothing else. We won't accept
  changes that reimplement the WhatsApp protocol, call private APIs, or automate, scrape or bulk-send
  messages. They break WhatsApp's terms and can get users' accounts banned.
- **Lightweight first.** No bundled browser engines and no heavy dependencies for small features. If
  a change adds noticeable size, memory or startup time, explain why it's worth it.
- **Privacy.** No telemetry, analytics, crash uploads or network calls of Wisp's own.
- **Cross-platform.** Features should work on Windows, macOS and Linux, or degrade gracefully where a
  platform can't support them.

For anything bigger than a small fix, please open an issue first so we can agree on the approach
before you invest time in it.

## Reporting bugs

Use the **Bug report** issue template. Include your OS and version, the Wisp version, how to reproduce
the problem, and what you expected to happen.

> [!IMPORTANT]
> **Never share chats, phone numbers, contact names or QR codes** in issues, logs or screenshots.
> Blur or crop anything personal. A QR code can be used to take over your session.

Security problems should **not** go in public issues. See [SECURITY.md](SECURITY.md).

## Suggesting features

Use the **Feature request** template and describe the problem you want solved, not just the solution.
Check existing issues first; a 👍 on an existing request helps us prioritize.

## Development setup

1. Install [Rust](https://rustup.rs) (stable, 1.90 or newer), with `rustfmt` and `clippy`:

   ```bash
   rustup component add rustfmt clippy
   ```

2. Install the platform prerequisites:

   - **Windows:** Visual Studio Build Tools with the "Desktop development with C++" workload. The
     WebView2 Runtime is already part of Windows 10 and 11.
   - **macOS:** Xcode Command Line Tools (`xcode-select --install`).
   - **Linux (Debian/Ubuntu):**

     ```bash
     sudo apt install pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev \
       libdbus-1-dev libayatana-appindicator3-dev
     ```

3. Fork the repository, clone your fork and run Wisp:

   ```bash
   git clone https://github.com/<you>/Wisp.git
   cd Wisp
   git switch dev
   cargo run
   ```

   Debug builds enable the webview's developer tools (right-click → Inspect), which is the easiest way
   to work on the theme or the injected script.

4. To use your build like an installed app on Windows (Start menu, desktop shortcut, Settings →
   Apps), build it and run the installer from the repository; run it again after each rebuild:

   ```bash
   cargo build --release
   "packaging\windows\Install Wisp.cmd"
   ```

## Project layout

```
src/
  main.rs         window, account pages, tray, menus and the event loop
  accounts.rs     accounts: settings, data folders, layout, the account bar's messages
  activity.rs     background throttling and memory trimming
  notify.rs       desktop notifications and Wisp's Windows app identity
  lock.rs         app lock state, PIN hashing, Windows Hello / Touch ID
  web.rs          URL policy, page <-> app messages, builds the injected script
  web/inject.js   notification bridge, shortcuts, theme loader, lock screen
  web/theme.css   the built-in theme: shapes and motion
  web/mono.js     recolors WhatsApp's styles to the mono palette
  web/privacy.css privacy blur rules
  web/lock.css    lock screen
  web/rail.html   account bar and the manage-accounts screen
  config.rs       settings file
  icon.rs         window and tray icons (embedded from assets/)
assets/           logo and generated icons (.png, .ico, .icns)
packaging/        Windows installer, macOS Info.plist, Linux .desktop file and installer
bench/            Windows benchmark against the Microsoft Store app
.github/          CI, release and repository automation
```

## Making changes

### Code style and checks

CI runs these on Windows, macOS and Linux, and all of them must pass:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Run them locally before pushing. A few conventions:

- Keep functions small and comment the *why*, not the *what*.
- Put platform-specific code behind `#[cfg(...)]` and keep the other platforms compiling.
- Add unit tests for logic that doesn't need a window (URL rules, parsing, state decisions); see the
  existing tests in `web.rs`, `activity.rs` and `icon.rs`.
- Treat everything coming from the page as untrusted. New IPC messages go through the `Ipc` enum in
  `web.rs` and are only accepted from `https://web.whatsapp.com`.

### Theme changes

WhatsApp Web's class names are generated and change often, so in `src/web/theme.css`:

- Target ids, roles, ARIA labels and `data-*` attributes, not class names.
- Scope every rule under `html.wisp-themed` so the theme can be switched off instantly.
- Colors come from `src/web/mono.js`, which recolors WhatsApp's own styles by value. Prefer adjusting
  its rules over hard-coding colors in `theme.css`, so the theme keeps working when WhatsApp changes.
- Prefer subtle changes that can't break the layout. WhatsApp's chat list is virtualized, so avoid
  vertical margins or heights on list rows.
- Check both WhatsApp's light and dark mode, and include before/after screenshots (with personal data
  blurred) in your pull request.

### Commit messages

We use [Conventional Commits](https://www.conventionalcommits.org):

```
feat: add a "mark all as read" tray action
fix(tray): show the unread badge after waking from sleep
docs: explain the Linux tray dependency
```

Common types are `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `ci` and `chore`. Keep the subject
under about 72 characters and use the body to explain why.

### Changelog

Add a line under **Unreleased** in [CHANGELOG.md](CHANGELOG.md) for any user-visible change.

## Branches and pull requests

| Branch | Purpose |
|---|---|
| `main` | Releases only. Every commit on `main` is a released version and is tagged. |
| `dev` | Default branch where all work is integrated. **Open your pull requests against `dev`.** |
| `feature/<name>`, `fix/<name>` | Your work, branched from `dev`. |
| `hotfix/<name>` | Maintainers only: an urgent fix branched from `main`, merged into `main` and back into `dev`. |

```bash
git switch dev && git pull upstream dev
git switch -c feature/my-change
# ...work, commit...
git push -u origin feature/my-change
```

Then open a pull request into `dev` and fill in the template. A good pull request:

- does one thing, with a clear title in Conventional Commit style;
- links the issue it closes (`Closes #123`);
- passes CI and includes tests where it makes sense;
- includes screenshots for visible changes.

A maintainer will review it, possibly ask for changes, and squash-merge it into `dev`. Pull requests
into `main` from any branch other than `dev` or `hotfix/*` are rejected automatically.

## Releases

Releases are cut by maintainers:

1. On `dev`: bump `version` in `Cargo.toml` (and `Cargo.lock`), move the **Unreleased** notes in
   `CHANGELOG.md` under the new version with today's date, update the comparison links at the
   bottom, and merge that.
2. Open a pull request from `dev` into `main` and merge it once CI is green.
3. Tag the merge commit on `main` and push the tag:

   ```bash
   git switch main && git pull
   git tag -a v0.3.0 -m "Wisp 0.3.0"
   git push origin v0.3.0
   ```

4. The **Release** workflow checks that the tag is on `main` and matches `Cargo.toml`, builds the
   Windows, macOS (universal) and Linux packages, and publishes a GitHub release whose notes are
   that version's `CHANGELOG.md` section, with checksums. If pushing the tag doesn't start it, run
   **Release** from the Actions tab and pick the tag.
5. Bring `dev` up to date with `main` (`git switch dev && git merge --ff-only main && git push`)
   so the next release starts from the released commit.

Thank you for helping make Wisp better!
