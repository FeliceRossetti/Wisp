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

  // ---- Theme -----------------------------------------------------------------
  let themeOn = !!CFG.theme;

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
    if (themeOn) addStyle("wisp-theme", CFG.css);
    else document.getElementById("wisp-theme")?.remove();
    if (CFG.userCss) {
      // Keep user.css last so it always wins over the built-in theme.
      const user = document.getElementById("wisp-user");
      if (user) user.parentNode.appendChild(user);
      else addStyle("wisp-user", CFG.userCss);
    }
  };

  if (document.documentElement) applyTheme();
  document.addEventListener("DOMContentLoaded", applyTheme, { once: true });

  // ---- Shortcuts -------------------------------------------------------------
  window.addEventListener(
    "keydown",
    (e) => {
      const mod = e.ctrlKey || e.metaKey;
      const key = e.key.toLowerCase();
      let handled = true;
      if (mod && !e.shiftKey && key === "q") post({ kind: "quit" });
      else if (mod && !e.shiftKey && key === "w") post({ kind: "hide" });
      else if ((mod && !e.shiftKey && key === "r") || e.key === "F5") location.reload();
      else if (mod && e.shiftKey && key === "t") {
        themeOn = !themeOn;
        applyTheme();
        post({ kind: "theme", on: themeOn });
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

  window.__wisp = { post, setTheme };
})();
