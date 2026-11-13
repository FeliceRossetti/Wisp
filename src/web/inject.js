// Runs in every WhatsApp Web document before its own scripts.
// It bridges notifications and shortcuts to the native shell and applies the theme.
(() => {
  if (window.__wisp) return;
  const CFG = __WISP_CONFIG__;

  __WISP_MONO__

  const post = (msg) => {
    try {
      window.ipc.postMessage(JSON.stringify(msg));
    } catch (_) {
      /* not in the shell */
    }
  };

  // ---- Notifications -> native desktop notifications -------------------------
  class WRNotification extends EventTarget {
    constructor(title, options = {}) {
      super();
      this.title = String(title ?? "");
      this.body = options.body != null ? String(options.body) : "";
      this.tag = options.tag != null ? String(options.tag) : "";
      this.data = options.data;
      this.icon = options.icon || "";
      this.silent = !!options.silent;
      this.onclick = this.onclose = this.onerror = this.onshow = null;
      post({ kind: "notify", title: this.title, body: this.body });
      setTimeout(() => {
        const ev = new Event("show");
        this.dispatchEvent(ev);
        if (typeof this.onshow === "function") this.onshow(ev);
      }, 0);
    }
    close() {}
    static get permission() {
      return "granted";
    }
    static requestPermission(cb) {
      if (typeof cb === "function") cb("granted");
      return Promise.resolve("granted");
    }
    static get maxActions() {
      return 0;
    }
  }
  Object.defineProperty(window, "Notification", {
    value: WRNotification,
    writable: true,
    configurable: true,
  });

  if (window.ServiceWorkerRegistration) {
    ServiceWorkerRegistration.prototype.showNotification = function (title, options = {}) {
      post({ kind: "notify", title: String(title ?? ""), body: options.body ? String(options.body) : "" });
      return Promise.resolve();
    };
  }

  if (navigator.permissions && navigator.permissions.query) {
    const query = navigator.permissions.query.bind(navigator.permissions);
    navigator.permissions.query = (desc) =>
      desc && desc.name === "notifications"
        ? Promise.resolve({ state: "granted", name: "notifications", onchange: null })
        : query(desc);
  }

  // ---- Theme and privacy blur -----------------------------------------------
  let themeOn = !!CFG.theme;
  let privateOn = !!CFG.privacy;

  const addStyle = (id, css) => {
    let el = document.getElementById(id);
    if (!el) {
      el = document.createElement("style");
      el.id = id;
      (document.head || document.documentElement).appendChild(el);
    }
    el.textContent = css;
  };

  // ---- Motion ----------------------------------------------------------------
  const EASE = "cubic-bezier(0.2, 0, 0, 1)";
  const EXIT = "cubic-bezier(0.3, 0, 0.8, 0.15)";
  const calm = () => window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches;
  // Crossfades for Wisp's own changes to the page (theme, accent, privacy blur).
  const MOTION_CSS = `
    ::view-transition-old(root), ::view-transition-new(root) {
      animation-duration: 260ms;
      animation-timing-function: ${EASE};
    }
    @media (prefers-reduced-motion: reduce) {
      ::view-transition-group(*), ::view-transition-old(*), ::view-transition-new(*) { animation: none !important; }
    }`;
  const smoothly = (change) => {
    if (document.startViewTransition && document.visibilityState === "visible" && document.body && !calm()) {
      try {
        document.startViewTransition(change);
        return;
      } catch (_) {
        /* fall through */
      }
    }
    change();
  };

  // The mono recolor (mono.js) is rebuilt whenever WhatsApp adds styles.
  let monoTimer = 0;
  const buildMono = () => {
    monoTimer = 0;
    if (themeOn) addStyle("wisp-mono", wispMono.build(CFG.accent));
    else document.getElementById("wisp-mono")?.remove();
  };
  const scheduleMono = () => {
    if (!monoTimer) monoTimer = setTimeout(buildMono, 150);
  };
  const insertRule = CSSStyleSheet.prototype.insertRule;
  CSSStyleSheet.prototype.insertRule = function (...args) {
    const result = insertRule.apply(this, args);
    if (themeOn && !(this.ownerNode && this.ownerNode.id && this.ownerNode.id.startsWith("wisp-"))) scheduleMono();
    return result;
  };
  const watchStyles = () => {
    new MutationObserver((changes) => {
      for (const change of changes) {
        for (const node of change.addedNodes) {
          if (node.id && node.id.startsWith("wisp-")) continue;
          if (node.tagName === "STYLE") scheduleMono();
          else if (node.tagName === "LINK" && /stylesheet/i.test(node.rel)) node.addEventListener("load", scheduleMono, { once: true });
        }
      }
    }).observe(document.head || document.documentElement, { childList: true, subtree: true });
  };

  // Mirror WhatsApp's own dark/light choice for Wisp's screens (lock, account bar).
  let lastDark = null;
  const syncAppearance = () => {
    if (!document.body) return;
    const dark = document.body.classList.contains("dark");
    document.documentElement.classList.toggle("wisp-dark", dark);
    document.documentElement.classList.toggle("wisp-light", !dark);
    if (dark !== lastDark) {
      lastDark = dark;
      post({ kind: "appearance", dark });
    }
  };

  const applyNow = () => {
    applyTheme();
    clearTimeout(monoTimer);
    buildMono();
  };

  const applyTheme = () => {
    const root = document.documentElement;
    if (!root) return;
    root.style.setProperty("--wisp-accent", CFG.accent);
    if (!document.getElementById("wisp-motion")) addStyle("wisp-motion", MOTION_CSS);
    root.classList.toggle("wisp-themed", themeOn);
    root.classList.toggle("wisp-private", privateOn);
    if (themeOn) addStyle("wisp-theme", CFG.css);
    else document.getElementById("wisp-theme")?.remove();
    scheduleMono();
    if (!document.getElementById("wisp-privacy")) addStyle("wisp-privacy", CFG.privacyCss);
    if (CFG.userCss) {
      // Keep user.css last so it always wins over the built-in theme.
      const user = document.getElementById("wisp-user");
      if (user) user.parentNode.appendChild(user);
      else addStyle("wisp-user", CFG.userCss);
    }
  };

  // ---- App lock ----------------------------------------------------------------
  // The app owns the lock state: the page only draws it and asks to unlock.
  // A page that may need locking starts locked, so chats never flash on load.
  let lock = { locked: !!CFG.appLock, setup: false, system: CFG.lockSystem || "", message: "", busy: false };
  let usePin = !lock.system;
  let lockEl = null;

  const el = (tag, props = {}, ...kids) => {
    const e = document.createElement(tag);
    Object.assign(e, props);
    e.append(...kids);
    return e;
  };
  const pinInput = (placeholder) =>
    el("input", { type: "password", inputMode: "numeric", autocomplete: "off", maxLength: 12, placeholder });
  const linkButton = (text, onclick) => el("button", { type: "button", className: "link", textContent: text, onclick });

  const shake = (node) => {
    if (calm() || !node.animate) return;
    node.animate(
      [{ transform: "translateX(0)" }, { transform: "translateX(-8px)" }, { transform: "translateX(7px)" },
        { transform: "translateX(-4px)" }, { transform: "translateX(2px)" }, { transform: "translateX(0)" }],
      { duration: 360, easing: "ease-out" },
    );
  };
  let shownMessage = "";
  const renderLock = () => {
    const root = document.documentElement;
    if (!root) return;
    const setupOnly = lock.setup && !lock.locked;
    root.classList.toggle("wisp-locked", lock.locked);
    root.classList.toggle("wisp-lock-setup", setupOnly);
    const was = lockEl && !lockEl.classList.contains("leaving") ? lockEl : null;
    const wasCard = was?.querySelector(".card");
    if (!lock.locked && !setupOnly) {
      // Fade the overlay out over the chats instead of cutting to them.
      if (was && !calm() && was.animate) {
        was.classList.add("leaving");
        was.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 240, easing: EXIT, fill: "forwards" }).finished.then(
          () => was.remove(),
          () => was.remove(),
        );
        wasCard?.animate([{ transform: "none" }, { transform: "scale(0.97)" }], { duration: 240, easing: EXIT, fill: "forwards" });
      } else {
        was?.remove();
      }
      document.querySelectorAll("#wisp-lock.leaving").forEach((n) => n !== was && n.remove());
      lockEl = null;
      shownMessage = "";
      return;
    }
    document.querySelectorAll("#wisp-lock").forEach((n) => n.remove());
    lockEl = null;
    if (!document.getElementById("wisp-lock-style")) addStyle("wisp-lock-style", CFG.lockCss);

    const message = el("p", { className: "message", textContent: lock.message || "" });
    message.setAttribute("role", "alert");
    const card = el("div", { className: "card" }, el("img", { src: CFG.logo, alt: "" }));
    let focus;

    if (setupOnly) {
      const first = pinInput("New PIN");
      const second = pinInput("Confirm PIN");
      const form = el("form", {}, first, second, el("button", { type: "submit", textContent: "Save PIN" }),
        linkButton("Cancel", () => post({ kind: "cancelsetup" })), message);
      form.onsubmit = (e) => {
        e.preventDefault();
        if (!/^\d{4,12}$/.test(first.value)) {
          message.textContent = "Use 4 to 12 digits.";
          shake(card);
        } else if (first.value !== second.value) {
          message.textContent = "The PINs don't match.";
          shake(card);
        } else post({ kind: "setpin", pin: first.value });
      };
      card.append(
        el("h1", { textContent: "Choose a PIN" }),
        el("p", {
          textContent: lock.system
            ? `You'll unlock Wisp with ${lock.system}. The PIN is a backup for when that isn't available.`
            : "You'll enter it to unlock Wisp.",
        }),
        form,
      );
      focus = first;
    } else if (lock.system && !usePin) {
      const button = el("button", {
        type: "submit",
        textContent: lock.busy ? `Waiting for ${lock.system}…` : `Unlock with ${lock.system}`,
        disabled: lock.busy,
      });
      const form = el("form", {}, button, linkButton("Use PIN instead", () => { usePin = true; renderLock(); }), message);
      form.onsubmit = (e) => {
        e.preventDefault();
        post({ kind: "unlock" });
      };
      card.append(el("h1", { textContent: "Wisp is locked" }), el("p", { textContent: "Your chats are hidden until you unlock." }), form);
      focus = button;
    } else {
      const input = pinInput("PIN");
      const kids = [input, el("button", { type: "submit", textContent: "Unlock" })];
      if (lock.system) kids.push(linkButton(`Use ${lock.system} instead`, () => { usePin = false; renderLock(); }));
      const form = el("form", {}, ...kids, message);
      form.onsubmit = (e) => {
        e.preventDefault();
        post({ kind: "pin", pin: input.value });
        input.value = "";
      };
      card.append(el("h1", { textContent: "Wisp is locked" }), el("p", { textContent: "Enter your PIN to unlock." }), form);
      focus = input;
    }

    // Only a lock screen that wasn't showing animates in; updates just redraw it.
    lockEl = el("div", { id: "wisp-lock", className: was ? "" : "appear" }, card);
    lockEl.setAttribute("role", "dialog");
    lockEl.setAttribute("aria-modal", "true");
    root.appendChild(lockEl);
    // A new message (a wrong PIN) gives the card a little shake.
    const text = lock.message || "";
    if (was && text && text !== shownMessage) shake(card);
    shownMessage = text;
    setTimeout(() => focus?.focus(), 0);
  };

  const setLock = (state) => {
    const wasLocked = lock.locked;
    lock = { ...lock, ...state };
    if (lock.locked && !wasLocked) usePin = !lock.system;
    renderLock();
  };

  if (document.documentElement) renderLock();
  document.addEventListener(
    "DOMContentLoaded",
    () => {
      renderLock();
      post({ kind: "ready" });
    },
    { once: true },
  );

  if (document.documentElement) applyTheme();
  document.addEventListener(
    "DOMContentLoaded",
    () => {
      applyTheme();
      watchStyles();
      syncAppearance();
      new MutationObserver(syncAppearance).observe(document.body, { attributes: true, attributeFilter: ["class"] });
    },
    { once: true },
  );
  window.addEventListener("load", scheduleMono, { once: true });

  // ---- Shortcuts -------------------------------------------------------------
  window.addEventListener(
    "keydown",
    (e) => {
      const mod = e.ctrlKey || e.metaKey;
      const key = e.key.toLowerCase();
      // While locked, keys only reach the lock screen (and Quit still works).
      if (lock.locked || (lock.setup && lockEl)) {
        e.stopPropagation();
        if (mod && !e.shiftKey && key === "q") {
          e.preventDefault();
          post({ kind: "quit" });
        } else if (!(e.target instanceof Node && lockEl?.contains(e.target))) {
          e.preventDefault();
          lockEl?.querySelector("input, button")?.focus();
        }
        return;
      }
      let handled = true;
      if (mod && !e.altKey && !e.shiftKey && /^[1-9]$/.test(e.key)) post({ kind: "switch", to: Number(e.key) });
      else if (e.ctrlKey && !e.altKey && e.key === "Tab") post({ kind: "cycle", forward: !e.shiftKey });
      else if (mod && !e.shiftKey && !e.altKey && e.key === ",") post({ kind: "settings" });
      else if (mod && e.shiftKey && key === "l") post({ kind: "lock" });
      else if (mod && !e.shiftKey && key === "q") post({ kind: "quit" });
      else if (mod && !e.shiftKey && key === "w") post({ kind: "hide" });
      else if ((mod && !e.shiftKey && key === "r") || e.key === "F5") location.reload();
      else if (mod && e.shiftKey && key === "t") {
        themeOn = !themeOn;
        smoothly(applyNow);
        post({ kind: "theme", on: themeOn });
      } else if (mod && e.shiftKey && key === "b") {
        privateOn = !privateOn;
        smoothly(applyNow);
        post({ kind: "privacy", on: privateOn });
      } else if (mod && (key === "=" || key === "+")) post({ kind: "zoom", dir: "in" });
      else if (mod && key === "-") post({ kind: "zoom", dir: "out" });
      else if (mod && key === "0") post({ kind: "zoom", dir: "reset" });
      else handled = false;
      if (handled) {
        e.preventDefault();
        e.stopPropagation();
      }
    },
    true,
  );

  // Only crossfade real changes: these are also sent when a page loads.
  const setTheme = (on) => {
    if (themeOn === !!on) return applyTheme();
    themeOn = !!on;
    smoothly(applyNow);
  };

  const setPrivacy = (on) => {
    if (privateOn === !!on) return applyTheme();
    privateOn = !!on;
    smoothly(applyNow);
  };

  const setAccent = (color) => {
    if (CFG.accent === String(color)) return;
    CFG.accent = String(color);
    // Updates --wisp-accent and rebuilds the mono recolor with the new accent.
    smoothly(applyNow);
  };

  // Switching to this account: fade it in from its own background.
  const enter = () => {
    if (lock.locked || calm() || !document.body || !document.documentElement.animate) return;
    let bg = getComputedStyle(document.body).backgroundColor;
    if (!bg || bg === "transparent" || bg === "rgba(0, 0, 0, 0)") bg = document.body.classList.contains("dark") ? "#0b141a" : "#ffffff";
    document.getElementById("wisp-enter")?.remove();
    const veil = el("div", { id: "wisp-enter" });
    veil.style.cssText = `position:fixed;inset:0;z-index:2147483646;pointer-events:none;background:${bg}`;
    document.documentElement.appendChild(veil);
    veil.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 220, easing: EASE, fill: "forwards" }).finished.then(
      () => veil.remove(),
      () => veil.remove(),
    );
  };

  window.__wisp = { post, setTheme, setPrivacy, setLock, setAccent, enter };
})();
