# Security policy

## Supported versions

Security fixes go into the latest release. Please make sure you can reproduce an issue on the newest
version before reporting it.

## Reporting a vulnerability

**Please don't report security problems in public issues.**

Report them privately through GitHub's
[private vulnerability reporting](https://github.com/FeliceRossetti/Wisp/security/advisories/new). Include:

- what the problem is and what an attacker could do with it;
- steps or a proof of concept to reproduce it;
- the Wisp version and operating system you tested on.

You can expect an acknowledgement within a few days. We'll keep you updated while we work on a fix,
and credit you in the release notes unless you'd prefer not to be named.

## Scope

In scope are problems in Wisp itself, for example:

- the page being able to send messages to the app that it shouldn't (IPC), or messages accepted from
  the wrong origin;
- navigation escaping `web.whatsapp.com`, or links handed to the operating system unsafely;
- downloads written outside the Downloads folder;
- one account in Wisp being able to read another account's session or messages;
- getting past app lock without Windows Hello, Touch ID or the PIN, or recovering the PIN from what
  Wisp stores;
- problems in how release builds are produced or published.

Out of scope are vulnerabilities in WhatsApp or WhatsApp Web (report those to
[Meta's bug bounty program](https://bugbounty.meta.com)), and in the system webviews (WebView2,
WKWebView, WebKitGTK), which should go to their vendors.
