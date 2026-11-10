// Runs in every WhatsApp Web document before its own scripts.
// It bridges notifications and shortcuts to the native shell and applies the theme.
(() => {
  if (window.__wisp) return;
  const CFG = __WISP_CONFIG__;

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

  const applyTheme = () => {
    const root = document.documentElement;
    if (!root) return;
    root.style.setProperty("--wisp-accent", CFG.accent);
    root.classList.toggle("wisp-themed", themeOn);
    root.classList.toggle("wisp-private", privateOn);
    if (themeOn) addStyle("wisp-theme", CFG.css);
    else document.getElementById("wisp-theme")?.remove();
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

  const renderLock = () => {
    const root = document.documentElement;
    if (!root) return;
    const setupOnly = lock.setup && !lock.locked;
    root.classList.toggle("wisp-locked", lock.locked);
    root.classList.toggle("wisp-lock-setup", setupOnly);
    lockEl?.remove();
    lockEl = null;
    if (!lock.locked && !setupOnly) return;
    if (!document.getElementById("wisp-lock-style")) addStyle("wisp-lock-style", CFG.lockCss);

    const message = el("p", { className: "message", textContent: lock.message || "" });
    message.setAttribute("role", "alert");
    const card = el("div", { className: "card" }, el("img", { src: CFG.logo, alt: "" }));
    let focus;

    if (setupOnly) {
      const first = pinInput("New PIN");
      const second = pinInput("Confirm PIN");
      const form = el("form", {}, first, second, el("button", { type: "submit", textContent: "Turn on app lock" }),
        linkButton("Cancel", () => post({ kind: "cancelsetup" })), message);
      form.onsubmit = (e) => {
        e.preventDefault();
        if (!/^\d{4,12}$/.test(first.value)) message.textContent = "Use 4 to 12 digits.";
        else if (first.value !== second.value) message.textContent = "The PINs don't match.";
        else post({ kind: "setpin", pin: first.value });
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

    lockEl = el("div", { id: "wisp-lock" }, card);
    lockEl.setAttribute("role", "dialog");
    lockEl.setAttribute("aria-modal", "true");
    root.appendChild(lockEl);
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
  document.addEventListener("DOMContentLoaded", applyTheme, { once: true });

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
      else if (mod && e.shiftKey && key === "l") post({ kind: "lock" });
      else if (mod && !e.shiftKey && key === "q") post({ kind: "quit" });
      else if (mod && !e.shiftKey && key === "w") post({ kind: "hide" });
      else if ((mod && !e.shiftKey && key === "r") || e.key === "F5") location.reload();
      else if (mod && e.shiftKey && key === "t") {
        themeOn = !themeOn;
        applyTheme();
        post({ kind: "theme", on: themeOn });
      } else if (mod && e.shiftKey && key === "b") {
        privateOn = !privateOn;
        applyTheme();
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

  const setTheme = (on) => {
    themeOn = !!on;
    applyTheme();
  };

  const setPrivacy = (on) => {
    privateOn = !!on;
    applyTheme();
  };

  window.__wisp = { post, setTheme, setPrivacy, setLock };
})();
