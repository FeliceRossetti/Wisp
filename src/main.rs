//! Wisp: a lightweight, themed desktop shell for WhatsApp Web.
//!
//! Not affiliated with WhatsApp or Meta. It loads the official web client in the
//! system webview (WebView2, WKWebView or WebKitGTK) and adds a native window,
//! tray icon, desktop notifications and a modern theme on top.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod accounts;
mod activity;
mod autostart;
mod config;
mod icon;
mod lock;
mod notify;
mod settings;
mod web;

use accounts::{Account, RailItem, RailMsg};
use anyhow::{Context, Result};
use config::{Config, Paths};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::{Window, WindowBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use web::{Ipc, ZoomDir};
use wry::dpi::{LogicalPosition, LogicalSize as WryLogicalSize};
use wry::{NewWindowResponse, PermissionKind, PermissionResponse, Rect, WebContext, WebView, WebViewBuilder};

const APP_NAME: &str = "Wisp";
const RAIL_HTML: &str = include_str!("web/rail.html");
/// Settings fades in over WhatsApp where the webview can draw see-through over
/// the page next to it. WebKitGTK can't, so on Linux it starts opaque.
const RAIL_SEE_THROUGH: bool = cfg!(any(windows, target_os = "macos"));

#[derive(Debug)]
enum UserEvent {
    /// A message from an account's WhatsApp page, tagged with the account id.
    Ipc(String, Ipc),
    /// An account's page title changed (it carries the unread count).
    Title(String, String),
    /// A message from the account rail.
    Rail(RailMsg),
    Tray(TrayIconEvent),
    Menu(MenuEvent),
    /// Result of a Windows Hello / Touch ID prompt.
    Auth(lock::SystemAuth),
    /// A notification for this account was clicked.
    NotificationClicked(String),
}

/// Ids of the tray menu entries, so menu events can be matched.
struct TrayMenu {
    show: MenuItem,
    settings: MenuItem,
    add_account: MenuItem,
    theme: CheckMenuItem,
    privacy: CheckMenuItem,
    notifications: CheckMenuItem,
    app_lock: CheckMenuItem,
    lock_now: MenuItem,
    reload: MenuItem,
    quit: MenuItem,
}

/// One account's WhatsApp page.
struct Page {
    id: String,
    // Declared before the context so it's dropped first.
    webview: WebView,
    _context: WebContext,
    unread: u32,
    /// Whether WhatsApp shows this account in dark mode (None until it says).
    dark: Option<bool>,
    activity: activity::Activity,
}

/// The account switcher. It only exists with 2+ accounts, and shares the data
/// folder of the first account so it doesn't start a browser process of its own.
struct Rail {
    // Declared before the context so it's dropped first.
    webview: WebView,
    _context: WebContext,
    /// The account whose data folder the rail shares.
    owner: String,
}

/// The window, plus the container child webviews live in on Linux.
struct Host {
    window: Window,
    #[cfg(target_os = "linux")]
    fixed: gtk::Fixed,
}

impl Host {
    fn new(window: Window) -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            use gtk::prelude::*;
            use tao::platform::unix::WindowExtUnix;
            let fixed = gtk::Fixed::new();
            let vbox = window
                .default_vbox()
                .context("no GTK container for the webviews")?;
            vbox.pack_start(&fixed, true, true, 0);
            fixed.show_all();
            Ok(Self { window, fixed })
        }
        #[cfg(not(target_os = "linux"))]
        Ok(Self { window })
    }

    /// Put a webview above the others. A view created later lands on top, so the
    /// bar (and Settings, which grows out of it) is raised again after a new
    /// account's page is created; otherwise Settings would open underneath it.
    fn raise(&self, webview: &WebView) {
        #[cfg(windows)]
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
            };
            use wry::WebViewExtWindows;
            // SAFETY: the handle is the webview's own child window, alive while `webview` is.
            let _ = unsafe {
                SetWindowPos(
                    webview.hwnd(),
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
        }
        #[cfg(target_os = "macos")]
        {
            use tao::platform::macos::WindowExtMacOS;
            use wry::WebViewExtMacOS;
            // Adding a subview again moves it to the front.
            let _ = webview.reparent(self.window.ns_window().cast());
        }
        #[cfg(target_os = "linux")]
        {
            use wry::WebViewExtUnix;
            // Re-adding a child draws it last, on top; the caller sets its bounds again.
            let _ = webview.reparent(&self.fixed);
        }
    }

    fn build(&self, builder: WebViewBuilder) -> wry::Result<WebView> {
        #[cfg(target_os = "linux")]
        {
            use wry::WebViewBuilderExtUnix;
            builder.build_gtk(&self.fixed)
        }
        #[cfg(not(target_os = "linux"))]
        builder.build_as_child(&self.window)
    }

    /// Bounds of the rail and of the account pages, in logical pixels.
    fn rects(&self, bar: bool, managing: bool) -> (Rect, Rect) {
        let size = self
            .window
            .inner_size()
            .to_logical::<f64>(self.window.scale_factor());
        let (rail_w, x, w) = accounts::layout(bar, size.width);
        let rect = |x: f64, w: f64| Rect {
            position: LogicalPosition::new(x, 0.0).into(),
            size: WryLogicalSize::new(w, size.height).into(),
        };
        let rail = if managing {
            rect(0.0, size.width)
        } else {
            rect(0.0, rail_w)
        };
        (rail, rect(x, w))
    }
}

/// Everything needed to create an account page while the app runs.
struct PageFactory {
    user_css: String,
    system_unlock: Option<&'static str>,
    default_dir: PathBuf,
    proxy: EventLoopProxy<UserEvent>,
}

impl PageFactory {
    fn build(
        &self,
        host: &Host,
        cfg: &Config,
        account: &Account,
        visible: bool,
        bounds: Rect,
    ) -> Result<Page> {
        let mut context = WebContext::new(Some(accounts::data_dir(&self.default_dir, &account.id)));
        let ipc_proxy = self.proxy.clone();
        let title_proxy = self.proxy.clone();
        let (ipc_id, title_id) = (account.id.clone(), account.id.clone());

        let mut builder = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(web::HOME)
            .with_bounds(bounds)
            .with_visible(visible)
            .with_initialization_script(web::init_script(&web::PageConfig {
                theme: cfg.theme_enabled,
                privacy: cfg.privacy_blur,
                accent: &cfg.accent,
                user_css: &self.user_css,
                locked: cfg.app_lock,
                lock_system: self.system_unlock.unwrap_or(""),
            }))
            .with_devtools(cfg!(debug_assertions))
            .with_clipboard(true)
            .with_autoplay(true)
            .with_ipc_handler(move |req| {
                if !web::is_trusted_origin(&req.uri().to_string()) {
                    return;
                }
                if let Some(msg) = Ipc::parse(req.body()) {
                    let _ = ipc_proxy.send_event(UserEvent::Ipc(ipc_id.clone(), msg));
                }
            })
            .with_document_title_changed_handler(move |title| {
                let _ = title_proxy.send_event(UserEvent::Title(title_id.clone(), title));
            })
            .with_navigation_handler(|url| {
                if web::is_internal(&url) {
                    true
                } else {
                    open_external(&url);
                    false
                }
            })
            .with_new_window_req_handler(|url, _features| {
                if web::is_internal(&url) && !url.starts_with("https://") {
                    // blob:/data: previews (images, PDFs) open in a normal window.
                    return NewWindowResponse::Allow;
                }
                open_external(&url);
                NewWindowResponse::Deny
            })
            .with_permission_handler(|kind| match kind {
                PermissionKind::Notifications
                | PermissionKind::Microphone
                | PermissionKind::Camera
                | PermissionKind::Autoplay => PermissionResponse::Allow,
                _ => PermissionResponse::Default,
            })
            .with_download_started_handler(|_url, path| {
                redirect_download(path);
                true
            });

        let ua = cfg.user_agent.trim();
        if !ua.is_empty() {
            builder = builder.with_user_agent(ua);
        } else {
            #[cfg(not(windows))]
            {
                builder = builder.with_user_agent(web::WEBKIT_UA);
            }
        }
        #[cfg(target_os = "macos")]
        if account.id != accounts::DEFAULT_ID {
            use wry::WebViewBuilderExtDarwin;
            builder = builder.with_data_store_identifier(accounts::store_id(&account.id));
        }

        let webview = host.build(builder).context("could not create the account page")?;
        let _ = webview.zoom(cfg.zoom);
        Ok(Page {
            id: account.id.clone(),
            webview,
            _context: context,
            unread: 0,
            dark: None,
            activity: activity::Activity::new(visible),
        })
    }

    fn build_rail(&self, host: &Host, cfg: &Config, owner: &str, bounds: Rect) -> Result<Rail> {
        let mut context = WebContext::new(Some(accounts::data_dir(&self.default_dir, owner)));
        let proxy = self.proxy.clone();
        let html = RAIL_HTML
            .replace("__ACCENT__", &web::css_color(&cfg.accent))
            .replace("__LOGO__", &web::logo_data_url())
            .replace("__SEE_THROUGH__", if RAIL_SEE_THROUGH { "true" } else { "false" });
        let builder = WebViewBuilder::new_with_web_context(&mut context)
            .with_html(html)
            .with_transparent(RAIL_SEE_THROUGH)
            .with_bounds(bounds)
            .with_visible(true)
            .with_devtools(cfg!(debug_assertions))
            .with_ipc_handler(move |req| {
                if let Some(msg) = RailMsg::parse(req.body()) {
                    let _ = proxy.send_event(UserEvent::Rail(msg));
                }
            })
            // The rail is our own page and never goes anywhere else.
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny);
        let webview = host.build(builder).context("could not create the account rail")?;
        Ok(Rail {
            webview,
            _context: context,
            owner: owner.to_string(),
        })
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{APP_NAME}: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let paths = Paths::resolve()?;
    let mut cfg = Config::load(&paths);
    accounts::normalize(&mut cfg.accounts, &mut cfg.active_account);
    let user_css = std::fs::read_to_string(&paths.user_css).unwrap_or_default();
    let system_unlock = lock::system_method();
    let background = std::env::args().any(|a| a == autostart::BACKGROUND_ARG);
    let start_hidden = cfg.start_hidden || background;
    let mut start_at_login = autostart::is_enabled();
    if cfg.app_lock && cfg.pin.is_none() && system_unlock.is_none() {
        // Nothing could unlock it; don't lock the user out of their own app.
        eprintln!("app lock: no PIN and no system unlock available; turning app lock off");
        cfg.app_lock = false;
    }
    remove_dirs_later(accounts::orphaned_dirs(&paths.webview_data, &cfg.accounts));
    if let Some(data_dir) = paths.webview_data.parent() {
        notify::init(data_dir);
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    forward_tray_and_menu_events(event_loop.create_proxy());

    // ---- Window ----------------------------------------------------------------
    let window_icon = {
        let i = icon::window();
        tao::window::Icon::from_rgba(i.data, i.width, i.height).ok()
    };
    let mut wb = WindowBuilder::new()
        .with_title(APP_NAME)
        .with_inner_size(LogicalSize::new(cfg.window.width, cfg.window.height))
        .with_min_inner_size(LogicalSize::new(560.0, 420.0))
        .with_maximized(cfg.window.maximized)
        .with_visible(false)
        .with_window_icon(window_icon);
    if let (Some(x), Some(y)) = (cfg.window.x, cfg.window.y) {
        let on_screen = event_loop.available_monitors().any(|m| {
            let (p, s) = (m.position(), m.size());
            x >= p.x && y >= p.y && x < p.x + s.width as i32 - 80 && y < p.y + s.height as i32 - 80
        });
        if on_screen {
            wb = wb.with_position(PhysicalPosition::new(x, y));
        }
    }
    let host = Host::new(wb.build(&event_loop).context("could not create the window")?)?;
    let window = &host.window;

    #[cfg(target_os = "macos")]
    let _app_menu = macos_app_menu();

    // ---- Account pages and the rail --------------------------------------------
    let factory = PageFactory {
        user_css,
        system_unlock,
        default_dir: paths.webview_data.clone(),
        proxy: event_loop.create_proxy(),
    };
    let mut active = cfg
        .accounts
        .iter()
        .position(|a| a.id == cfg.active_account)
        .unwrap_or(0);
    let (_, page_rect) = host.rects(accounts::show_bar(cfg.accounts.len(), cfg.account_bar), false);
    let mut pages = Vec::new();
    for (i, account) in cfg.accounts.iter().enumerate() {
        pages.push(factory.build(&host, &cfg, account, i == active, page_rect)?);
    }
    // Created at the end of the first loop iteration when there are 2+ accounts.
    let mut rail: Option<Rail> = None;

    if !start_hidden {
        window.set_visible(true);
        window.set_focus();
    }

    // ---- Event loop ------------------------------------------------------------
    let mut tray: Option<(TrayIcon, TrayMenu)> = None;
    // Ask rather than assume: a window that opens behind others never gets the
    // focus events that would otherwise tell us.
    let mut focused = window.is_focused();
    // What the app has shown or hidden. tao applies visibility changes a moment later,
    // so asking the window right after a change can give a stale answer.
    let mut shown = !start_hidden;
    let mut minimized = false;
    let mut managing = false;
    // Settings reopens once the PIN screen it opened is done.
    let mut back_to_settings = false;
    // Whether keyboard focus was last handed to Settings (true) or the page.
    let mut focus_in_settings = false;
    // Account pages there were when the bar was last put on top.
    let mut stacked_pages = 0;
    // Session folders of removed accounts that the Settings view still has
    // open (it shares the first account's folder); deleted once it closes.
    let mut rail_leftovers: Vec<PathBuf> = Vec::new();
    let mut lock_state = lock::LockState::new(cfg.app_lock, system_unlock);
    let proxy = event_loop.create_proxy();

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        let window = &host.window;
        let mut quit = false;
        // Redraw the rail / re-place webviews at the end of this iteration.
        let mut rail_dirty = false;
        let mut layout_dirty = false;

        match event {
            Event::NewEvents(StartCause::Init) => {
                // The window has its final (e.g. maximized) size only now.
                layout_dirty = true;
                // The tray must be created once the loop is running (GTK needs it on Linux).
                match build_tray(&cfg) {
                    Ok(t) => tray = Some(t),
                    Err(e) => {
                        eprintln!("tray: unavailable ({e:#}); closing the window will quit");
                        show_window(window, &mut shown);
                    }
                }
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    if cfg.close_to_tray && tray.is_some() {
                        hide_window(window, &mut shown);
                    } else {
                        quit = true;
                    }
                }
                WindowEvent::Focused(f) => {
                    focused = f;
                    if let Some(p) = pages.get_mut(active) {
                        p.activity.set_focused(f);
                    }
                }
                WindowEvent::Resized(size) => {
                    minimized = window.is_minimized();
                    if !window.is_maximized() && !window.is_minimized() && size.width > 0 {
                        let logical = size.to_logical::<f64>(window.scale_factor());
                        cfg.window.width = logical.width;
                        cfg.window.height = logical.height;
                    }
                    cfg.window.maximized = window.is_maximized();
                    layout_dirty = true;
                }
                WindowEvent::ScaleFactorChanged { .. } => layout_dirty = true,
                WindowEvent::Moved(pos) if !window.is_maximized() && !window.is_minimized() => {
                    cfg.window.x = Some(pos.x);
                    cfg.window.y = Some(pos.y);
                }
                _ => {}
            },

            Event::Reopen { .. } => show_window(window, &mut shown),

            Event::UserEvent(UserEvent::Title(id, title)) => {
                if let Some(p) = pages.iter_mut().find(|p| p.id == id) {
                    let n = web::unread_from_title(&title);
                    if n != p.unread {
                        p.unread = n;
                        let total = pages.iter().map(|p| p.unread).sum();
                        update_unread(window, tray.as_ref().map(|t| &t.0), total);
                        rail_dirty = true;
                    }
                }
            }

            Event::UserEvent(UserEvent::Ipc(id, msg)) => {
                let Some(from) = pages.iter().position(|p| p.id == id) else {
                    return;
                };
                match msg {
                    Ipc::Notify { title, body } => {
                        let looking = focused && shown && !minimized && from == active && !managing;
                        if cfg.notifications && !looking {
                            let name = cfg.accounts.get(from).map(|a| a.name.as_str());
                            let many = cfg.accounts.len() > 1;
                            let (title, body) = notification_content(
                                &cfg,
                                lock_state.locked,
                                title,
                                body,
                                name.filter(|_| many),
                            );
                            let p = proxy.clone();
                            notify::show(title, body, move || {
                                let _ = p.send_event(UserEvent::NotificationClicked(id.clone()));
                            });
                        }
                    }
                    Ipc::Quit => quit = true,
                    Ipc::Hide => {
                        if tray.is_some() {
                            hide_window(window, &mut shown);
                        } else {
                            window.set_minimized(true);
                        }
                    }
                    Ipc::Theme { on } => {
                        cfg.theme_enabled = on;
                        if let Some((_, m)) = &tray {
                            m.theme.set_checked(on);
                        }
                        broadcast(&pages, &format!("window.__wisp && window.__wisp.setTheme({on})"));
                        cfg.save(&paths);
                        rail_dirty = true;
                    }
                    Ipc::Privacy { on } => {
                        cfg.privacy_blur = on;
                        if let Some((_, m)) = &tray {
                            m.privacy.set_checked(on);
                        }
                        broadcast(
                            &pages,
                            &format!("window.__wisp && window.__wisp.setPrivacy({on})"),
                        );
                        cfg.save(&paths);
                        rail_dirty = true;
                    }
                    Ipc::Ready => {
                        // A page (re)loaded: bring it up to date with what changed since start.
                        let p = &pages[from].webview;
                        let _ = p.evaluate_script(&format!(
                            "window.__wisp && (window.__wisp.setAccent({:?}), window.__wisp.setTheme({}), window.__wisp.setPrivacy({}))",
                            cfg.accent, cfg.theme_enabled, cfg.privacy_blur
                        ));
                        let _ = p.evaluate_script(&lock_state.script());
                    }
                    Ipc::Lock => {
                        if cfg.app_lock {
                            lock_state.lock();
                            managing = false;
                            back_to_settings = false;
                            push_lock(&pages, &lock_state);
                            rail_dirty = true;
                            layout_dirty = true;
                        }
                    }
                    Ipc::Unlock => {
                        if lock_state.start_system_auth() {
                            let p = proxy.clone();
                            lock::request_system_auth(window, move |r| {
                                let _ = p.send_event(UserEvent::Auth(r));
                            });
                            push_lock(&pages, &lock_state);
                        }
                    }
                    Ipc::Pin { pin } => {
                        lock_state.try_pin(cfg.pin.as_ref(), &pin, Instant::now());
                        push_lock(&pages, &lock_state);
                        rail_dirty = true;
                    }
                    Ipc::SetPin { pin } => {
                        // Only accepted while the "choose a PIN" screen is open.
                        if lock_state.setup && !lock_state.locked && lock::valid_pin(&pin) {
                            cfg.pin = Some(lock::PinHash::new(&pin));
                            cfg.app_lock = true;
                            lock_state.end_setup();
                            cfg.save(&paths);
                            push_lock(&pages, &lock_state);
                            if std::mem::take(&mut back_to_settings) {
                                managing = true;
                                layout_dirty = true;
                            }
                            rail_dirty = true;
                        }
                    }
                    Ipc::CancelSetup => {
                        lock_state.end_setup();
                        push_lock(&pages, &lock_state);
                        if std::mem::take(&mut back_to_settings) {
                            managing = true;
                            layout_dirty = true;
                            rail_dirty = true;
                        }
                    }
                    Ipc::Zoom { dir } => {
                        cfg.zoom = match dir {
                            ZoomDir::In => Config::clamp_zoom(cfg.zoom + 0.1),
                            ZoomDir::Out => Config::clamp_zoom(cfg.zoom - 0.1),
                            ZoomDir::Reset => 1.0,
                        };
                        for p in &pages {
                            let _ = p.webview.zoom(cfg.zoom);
                        }
                        cfg.save(&paths);
                        rail_dirty = true;
                    }
                    Ipc::Settings => {
                        if !lock_state.locked {
                            managing = true;
                            layout_dirty = true;
                            rail_dirty = true;
                        }
                    }
                    Ipc::Appearance { dark } => {
                        pages[from].dark = Some(dark);
                        rail_dirty = from == active;
                    }
                    Ipc::Switch { to } => {
                        if !lock_state.locked && (1..=pages.len()).contains(&to) {
                            switch_to(&mut pages, &mut active, to - 1, focused, &mut cfg);
                            rail_dirty = true;
                        }
                    }
                    Ipc::Cycle { forward } => {
                        if !lock_state.locked && pages.len() > 1 {
                            let n = pages.len();
                            let next = if forward {
                                (active + 1) % n
                            } else {
                                (active + n - 1) % n
                            };
                            switch_to(&mut pages, &mut active, next, focused, &mut cfg);
                            rail_dirty = true;
                        }
                    }
                }
            }

            Event::UserEvent(UserEvent::Rail(msg)) => {
                // Nothing in the rail works while Wisp is locked.
                if lock_state.locked && msg != RailMsg::Ready {
                    rail_dirty = true;
                } else {
                    match msg {
                        RailMsg::Ready => rail_dirty = true,
                        RailMsg::Select { id } => {
                            if let Some(i) = pages.iter().position(|p| p.id == id) {
                                switch_to(&mut pages, &mut active, i, focused, &mut cfg);
                                managing = false;
                                layout_dirty = true;
                                rail_dirty = true;
                            }
                        }
                        RailMsg::Add => {
                            add_account(
                                &factory,
                                &host,
                                &mut cfg,
                                &mut pages,
                                &mut active,
                                focused,
                                &paths,
                            );
                            managing = false;
                            layout_dirty = true;
                            rail_dirty = true;
                        }
                        RailMsg::Manage { open } => {
                            managing = open;
                            layout_dirty = true;
                            rail_dirty = true;
                        }
                        RailMsg::Rename { id, name } => {
                            if let (Some(a), Some(name)) = (
                                cfg.accounts.iter_mut().find(|a| a.id == id),
                                accounts::clean_name(&name),
                            ) {
                                a.name = name;
                                cfg.save(&paths);
                            }
                            rail_dirty = true;
                        }
                        RailMsg::Move { id, up } => {
                            if accounts::move_account(&mut cfg.accounts, &id, up) {
                                let current = pages[active].id.clone();
                                pages.sort_by_key(|p| cfg.accounts.iter().position(|a| a.id == p.id));
                                active = pages.iter().position(|p| p.id == current).unwrap_or(0);
                                cfg.save(&paths);
                            }
                            rail_dirty = true;
                        }
                        RailMsg::Set { setting } => {
                            use settings::Setting;
                            match setting {
                                Setting::Theme(on) => {
                                    cfg.theme_enabled = on;
                                    broadcast(
                                        &pages,
                                        &format!("window.__wisp && window.__wisp.setTheme({on})"),
                                    );
                                }
                                Setting::Accent(color) => {
                                    if settings::valid_accent(&color) {
                                        broadcast(
                                            &pages,
                                            &format!("window.__wisp && window.__wisp.setAccent({:?})", color),
                                        );
                                        cfg.accent = color;
                                    }
                                }
                                Setting::Zoom(z) => {
                                    cfg.zoom = Config::clamp_zoom(z);
                                    for p in &pages {
                                        let _ = p.webview.zoom(cfg.zoom);
                                    }
                                }
                                Setting::AccountBar(on) => {
                                    cfg.account_bar = on;
                                    layout_dirty = true;
                                }
                                Setting::PrivacyBlur(on) => {
                                    cfg.privacy_blur = on;
                                    broadcast(
                                        &pages,
                                        &format!("window.__wisp && window.__wisp.setPrivacy({on})"),
                                    );
                                }
                                Setting::Notifications(on) => cfg.notifications = on,
                                Setting::NotificationPreview(on) => cfg.notification_preview = on,
                                Setting::AppLock(false) => {
                                    cfg.app_lock = false;
                                    cfg.pin = None;
                                }
                                // Turning it on goes through choosing a PIN (RailMsg::LockSetup).
                                Setting::AppLock(true) => {}
                                Setting::LockAfterMinutes(m) => {
                                    if settings::LOCK_AFTER.contains(&m) {
                                        cfg.lock_after_minutes = m;
                                    }
                                }
                                Setting::CloseToTray(on) => cfg.close_to_tray = on,
                                Setting::StartHidden(on) => cfg.start_hidden = on,
                                Setting::StartAtLogin(on) => {
                                    if let Err(e) = autostart::set_enabled(on) {
                                        eprintln!("start at sign-in: {e}");
                                    }
                                    start_at_login = autostart::is_enabled();
                                }
                            }
                            cfg.save(&paths);
                            rail_dirty = true;
                        }
                        RailMsg::LockSetup => {
                            lock_state.begin_setup();
                            back_to_settings = managing;
                            managing = false;
                            layout_dirty = true;
                            rail_dirty = true;
                            push_lock(&pages, &lock_state);
                        }
                        RailMsg::Open { link } => {
                            if let Some(url) = settings::link(&link) {
                                open_external(url);
                            }
                        }
                        RailMsg::Remove { id } => {
                            // Settings stays open. If its view uses this account's
                            // folder, the folder is deleted once Settings closes.
                            let owns = rail.as_ref().is_some_and(|r| r.owner == id);
                            if owns && !managing {
                                rail = None;
                            }
                            if let Some(dir) =
                                remove_account(&mut cfg, &mut pages, &mut active, &id, focused, &paths)
                            {
                                if owns && managing {
                                    rail_leftovers.push(dir);
                                } else {
                                    remove_dirs_later(vec![dir]);
                                }
                            }
                            let total = pages.iter().map(|p| p.unread).sum();
                            update_unread(window, tray.as_ref().map(|t| &t.0), total);
                            layout_dirty = true;
                            rail_dirty = true;
                        }
                    }
                }
            }

            // Clicking the tray icon takes focus from the window, so a click always shows it.
            Event::UserEvent(UserEvent::Tray(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            })) => show_window(window, &mut shown),

            // Clicking a notification opens Wisp on the account it came from.
            Event::UserEvent(UserEvent::NotificationClicked(id)) => {
                show_window(window, &mut shown);
                if !lock_state.locked
                    && let Some(i) = pages.iter().position(|p| p.id == id)
                {
                    switch_to(&mut pages, &mut active, i, focused, &mut cfg);
                    managing = false;
                    layout_dirty = true;
                    rail_dirty = true;
                }
            }

            Event::UserEvent(UserEvent::Auth(result)) => {
                lock_state.finish_system_auth(result);
                push_lock(&pages, &lock_state);
                rail_dirty = true;
            }

            Event::UserEvent(UserEvent::Menu(ev)) => {
                if let Some((_, m)) = &tray {
                    let id = ev.id();
                    if id == m.show.id() {
                        show_window(window, &mut shown);
                    } else if id == m.settings.id() {
                        if !lock_state.locked {
                            managing = true;
                            show_window(window, &mut shown);
                            layout_dirty = true;
                            rail_dirty = true;
                        }
                    } else if id == m.add_account.id() {
                        if !lock_state.locked {
                            add_account(
                                &factory,
                                &host,
                                &mut cfg,
                                &mut pages,
                                &mut active,
                                focused,
                                &paths,
                            );
                            managing = false;
                            show_window(window, &mut shown);
                            layout_dirty = true;
                            rail_dirty = true;
                        }
                    } else if id == m.reload.id() {
                        if let Some(p) = pages.get(active) {
                            let _ = p.webview.reload();
                        }
                    } else if id == m.theme.id() {
                        cfg.theme_enabled = m.theme.is_checked();
                        broadcast(
                            &pages,
                            &format!("window.__wisp && window.__wisp.setTheme({})", cfg.theme_enabled),
                        );
                        cfg.save(&paths);
                        rail_dirty = true;
                    } else if id == m.privacy.id() {
                        cfg.privacy_blur = m.privacy.is_checked();
                        broadcast(
                            &pages,
                            &format!("window.__wisp && window.__wisp.setPrivacy({})", cfg.privacy_blur),
                        );
                        cfg.save(&paths);
                        rail_dirty = true;
                    } else if id == m.app_lock.id() {
                        if lock_state.locked {
                            // Can't be changed from the tray while locked.
                        } else if cfg.app_lock {
                            cfg.app_lock = false;
                            cfg.pin = None;
                            cfg.save(&paths);
                            rail_dirty = true;
                        } else {
                            // Turned on once a PIN is chosen in the window.
                            lock_state.begin_setup();
                            back_to_settings = managing;
                            managing = false;
                            layout_dirty = true;
                            show_window(window, &mut shown);
                            push_lock(&pages, &lock_state);
                        }
                    } else if id == m.lock_now.id() {
                        if cfg.app_lock {
                            lock_state.lock();
                            managing = false;
                            back_to_settings = false;
                            push_lock(&pages, &lock_state);
                            layout_dirty = true;
                            rail_dirty = true;
                        }
                    } else if id == m.notifications.id() {
                        cfg.notifications = m.notifications.is_checked();
                        cfg.save(&paths);
                        rail_dirty = true;
                    } else if id == m.quit.id() {
                        quit = true;
                    }
                }
            }

            _ => {}
        }

        // Lock after a while away.
        let now = Instant::now();
        let in_use = shown && !minimized && focused;
        lock_state.set_away(!in_use, now);
        let (locked_now, lock_due) = lock_state.check_idle(cfg.app_lock, cfg.lock_after_minutes, now);
        if locked_now {
            managing = false;
            back_to_settings = false;
            push_lock(&pages, &lock_state);
            rail_dirty = true;
            layout_dirty = true;
        }

        let want_rail = accounts::show_bar(pages.len(), cfg.account_bar) || managing;
        // A new view for the bar once its account is gone, but not while Settings is open in it.
        if want_rail != rail.is_some() || (!managing && rail.as_ref().is_some_and(|r| r.owner != pages[0].id)) {
            rail = None;
            remove_dirs_later(std::mem::take(&mut rail_leftovers));
            if want_rail {
                let (rail_rect, _) = host.rects(want_rail, managing);
                match factory.build_rail(&host, &cfg, &pages[0].id, rail_rect) {
                    Ok(r) => rail = Some(r),
                    Err(e) => eprintln!("{e:#}"),
                }
            }
            layout_dirty = true;
            rail_dirty = true;
        }
        // A new account's page covers the bar until the bar is raised again.
        if pages.len() != stacked_pages {
            stacked_pages = pages.len();
            if let Some(r) = &rail {
                host.raise(&r.webview);
                layout_dirty = true;
            }
        }
        if layout_dirty {
            let (rail_rect, page_rect) = host.rects(want_rail, managing);
            if let Some(r) = &rail {
                let _ = r.webview.set_bounds(rail_rect);
            }
            for p in &pages {
                let _ = p.webview.set_bounds(page_rect);
            }
        }
        if let (true, Some(r)) = (rail_dirty, &rail) {
            let items: Vec<RailItem> = cfg
                .accounts
                .iter()
                .zip(&pages)
                .enumerate()
                .map(|(i, (account, p))| RailItem {
                    account,
                    unread: p.unread,
                    active: i == active,
                })
                .collect();
            let _ = r.webview.evaluate_script(&accounts::rail_script(
                &items,
                lock_state.locked,
                managing,
                pages.get(active).and_then(|p| p.dark),
                &settings::view(&cfg, start_at_login, system_unlock),
            ));
        }

        // Keys go to Settings while it's open, and back to the chat after.
        if managing != focus_in_settings {
            focus_in_settings = managing;
            if managing {
                if let Some(r) = &rail {
                    let _ = r.webview.focus();
                }
            } else if let Some(p) = pages.get(active) {
                let _ = p.webview.focus();
            }
        }

        // Only the account in front is visible; the others rest like a minimized window.
        let window_up = shown && !minimized;
        let mut wake = lock_due;
        for (i, p) in pages.iter_mut().enumerate() {
            let at = p
                .activity
                .update(&p.webview, window_up && i == active);
            wake = match (wake, at) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        if let Some(at) = wake {
            *control_flow = ControlFlow::WaitUntil(at);
        }
        if let Some((_, m)) = &tray {
            m.theme.set_checked(cfg.theme_enabled);
            m.privacy.set_checked(cfg.privacy_blur);
            m.notifications.set_checked(cfg.notifications);
            m.app_lock.set_checked(cfg.app_lock);
            m.app_lock.set_enabled(!lock_state.locked);
            m.lock_now.set_enabled(cfg.app_lock && !lock_state.locked);
            m.add_account
                .set_enabled(!lock_state.locked && pages.len() < accounts::max_accounts());
        }

        if quit {
            cfg.save(&paths);
            if !rail_leftovers.is_empty() {
                // Whatever can't be deleted now is cleaned up on the next start.
                rail = None;
                remove_dirs_now(std::mem::take(&mut rail_leftovers));
            }
            tray = None;
            *control_flow = ControlFlow::Exit;
        }
    });
}

/// Bring account `to` to the front.
fn switch_to(pages: &mut [Page], active: &mut usize, to: usize, focused: bool, cfg: &mut Config) {
    if to >= pages.len() {
        return;
    }
    if *active != to {
        let _ = pages[to]
            .webview
            .evaluate_script("window.__wisp && window.__wisp.enter()");
    }
    *active = to;
    pages[to].activity.set_focused(focused);
    if let Some(a) = cfg.accounts.get(to) {
        cfg.active_account = a.id.clone();
    }
    let _ = pages[to].webview.focus();
}

fn add_account(
    factory: &PageFactory,
    host: &Host,
    cfg: &mut Config,
    pages: &mut Vec<Page>,
    active: &mut usize,
    focused: bool,
    paths: &Paths,
) {
    if pages.len() >= accounts::max_accounts() {
        return;
    }
    let account = accounts::new_account(&cfg.accounts);
    let (_, page_rect) = host.rects(true, false);
    match factory.build(host, cfg, &account, false, page_rect) {
        Ok(page) => {
            cfg.accounts.push(account);
            pages.push(page);
            let last = pages.len() - 1;
            switch_to(pages, active, last, focused, cfg);
            cfg.save(paths);
        }
        Err(e) => eprintln!("could not add an account: {e:#}"),
    }
}

/// Remove an account. The last account can't be removed. Returns its session
/// folder, for the caller to delete.
fn remove_account(
    cfg: &mut Config,
    pages: &mut Vec<Page>,
    active: &mut usize,
    id: &str,
    focused: bool,
    paths: &Paths,
) -> Option<PathBuf> {
    if pages.len() < 2 {
        return None;
    }
    let i = pages.iter().position(|p| p.id == id)?;
    let current = pages[*active].id.clone();
    drop(pages.remove(i));
    cfg.accounts.retain(|a| a.id != id);
    let next = pages.iter().position(|p| p.id == current).unwrap_or(0);
    switch_to(pages, active, next, focused, cfg);
    cfg.save(paths);

    // Sign the account out of Wisp for good.
    #[cfg(target_os = "macos")]
    if accounts::separate_sessions_supported() {
        use wry::WebViewExtDarwin;
        WebView::remove_data_store(&accounts::store_id(id), |_| {});
    }
    Some(accounts::data_dir(&paths.webview_data, id))
}

/// Delete session folders in the background. A webview can hold its files for
/// a moment after it closes; anything still left is removed on the next start.
fn remove_dirs_later(dirs: Vec<PathBuf>) {
    if dirs.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for dir in dirs {
            for _ in 0..20 {
                if !dir.exists() || std::fs::remove_dir_all(&dir).is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    });
}

/// Delete session folders before quitting, giving webviews a moment to let go.
fn remove_dirs_now(dirs: Vec<PathBuf>) {
    for dir in dirs {
        for _ in 0..6 {
            if !dir.exists() || std::fs::remove_dir_all(&dir).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
    }
}

fn broadcast(pages: &[Page], script: &str) {
    for p in pages {
        let _ = p.webview.evaluate_script(script);
    }
}

fn push_lock(pages: &[Page], state: &lock::LockState) {
    broadcast(pages, &state.script());
}

fn forward_tray_and_menu_events(proxy: EventLoopProxy<UserEvent>) {
    let p = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |e| {
        let _ = p.send_event(UserEvent::Tray(e));
    }));
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy.send_event(UserEvent::Menu(e));
    }));
}

fn build_tray(cfg: &Config) -> Result<(TrayIcon, TrayMenu)> {
    let m = TrayMenu {
        show: MenuItem::new(format!("Show {APP_NAME}"), true, None),
        settings: MenuItem::new("Settings", true, None),
        add_account: MenuItem::new("Add account", cfg.accounts.len() < accounts::max_accounts(), None),
        theme: CheckMenuItem::new("Wisp theme", true, cfg.theme_enabled, None),
        privacy: CheckMenuItem::new("Privacy blur", true, cfg.privacy_blur, None),
        app_lock: CheckMenuItem::new("App lock", true, cfg.app_lock, None),
        lock_now: MenuItem::new("Lock now", cfg.app_lock, None),
        notifications: CheckMenuItem::new("Notifications", true, cfg.notifications, None),
        reload: MenuItem::new("Reload", true, None),
        quit: MenuItem::new("Quit", true, None),
    };
    let menu = Menu::new();
    menu.append_items(&[
        &m.show,
        &m.add_account,
        &m.settings,
        &PredefinedMenuItem::separator(),
        &m.theme,
        &m.privacy,
        &m.app_lock,
        &m.lock_now,
        &m.notifications,
        &m.reload,
        &PredefinedMenuItem::separator(),
        &m.quit,
    ])?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip(APP_NAME)
        .with_icon(tray_icon_image(false)?)
        .build()?;
    Ok((tray, m))
}

fn tray_icon_image(badge: bool) -> Result<tray_icon::Icon> {
    let i = icon::tray(badge);
    Ok(tray_icon::Icon::from_rgba(i.data, i.width, i.height)?)
}

fn update_unread(window: &Window, tray: Option<&TrayIcon>, n: u32) {
    let label = if n > 0 {
        format!("{APP_NAME} ({n})")
    } else {
        APP_NAME.to_string()
    };
    window.set_title(&label);
    if let Some(tray) = tray {
        let _ = tray.set_tooltip(Some(if n > 0 {
            format!("{n} unread")
        } else {
            APP_NAME.to_string()
        }));
        if let Ok(icon) = tray_icon_image(n > 0) {
            let _ = tray.set_icon(Some(icon));
        }
    }
}

fn show_window(window: &Window, shown: &mut bool) {
    window.set_visible(true);
    window.set_minimized(false);
    window.set_focus();
    *shown = true;
}

fn hide_window(window: &Window, shown: &mut bool) {
    window.set_visible(false);
    *shown = false;
}

/// What a notification may show: nothing personal (not even the account) while
/// privacy blur or app lock is on, and no message text when previews are off.
/// With several accounts, the title says which one the message is for.
fn notification_content(
    cfg: &Config,
    locked: bool,
    title: String,
    body: String,
    account: Option<&str>,
) -> (String, String) {
    if cfg.privacy_blur || locked {
        return (APP_NAME.to_string(), "New message".to_string());
    }
    let title = match account {
        Some(name) => accounts::notification_title(name, &title, true),
        None => title,
    };
    if cfg.notification_preview {
        (title, body)
    } else {
        (title, String::new())
    }
}

fn open_external(url: &str) {
    if web::is_openable_externally(url)
        && let Err(e) = open::that_detached(url)
    {
        eprintln!("could not open {url}: {e}");
    }
}

/// Save downloads in the user's Downloads folder, without overwriting anything.
fn redirect_download(path: &mut PathBuf) {
    let Some(name) = path.file_name().map(|n| n.to_owned()) else {
        return;
    };
    let Some(dir) = directories::UserDirs::new().and_then(|u| u.download_dir().map(Path::to_path_buf)) else {
        return;
    };
    *path = unique_path(&dir, Path::new(&name));
}

fn unique_path(dir: &Path, name: &Path) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.file_stem().unwrap_or_default().to_string_lossy();
    let ext = name
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// macOS needs a real menu bar for Cmd+C / Cmd+V / Cmd+Q to work in the webview.
#[cfg(target_os = "macos")]
fn macos_app_menu() -> Menu {
    use tray_icon::menu::Submenu;
    let menu = Menu::new();
    let app = Submenu::new(APP_NAME, true);
    let _ = app.append_items(&[
        &PredefinedMenuItem::about(None, None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None),
    ]);
    let edit = Submenu::new("Edit", true);
    let _ = edit.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
    ]);
    let win = Submenu::new("Window", true);
    let _ = win.append_items(&[
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::fullscreen(None),
    ]);
    let _ = menu.append_items(&[&app, &edit, &win]);
    menu.init_for_nsapp();
    menu
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_respect_privacy_settings() {
        let mut cfg = Config::default();
        let msg = || ("Ann".to_string(), "see you at 8".to_string());
        let (t, b) = msg();
        assert_eq!(notification_content(&cfg, false, t, b, None), msg());
        let (t, b) = msg();
        assert_eq!(
            notification_content(&cfg, false, t, b, Some("Work")),
            ("Work · Ann".into(), "see you at 8".into())
        );
        cfg.notification_preview = false;
        let (t, b) = msg();
        assert_eq!(
            notification_content(&cfg, false, t, b, None),
            ("Ann".into(), String::new())
        );
        let (t, b) = msg();
        assert_eq!(
            notification_content(&cfg, true, t, b, Some("Work")),
            (APP_NAME.into(), "New message".into()),
            "locked: not even the account name"
        );
        cfg.privacy_blur = true;
        let (t, b) = msg();
        assert_eq!(
            notification_content(&cfg, false, t, b, Some("Work")),
            (APP_NAME.into(), "New message".into())
        );
    }

    #[test]
    fn unique_path_avoids_overwrites() {
        let dir = std::env::temp_dir().join(format!("wisp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        assert_eq!(unique_path(&dir, Path::new("b.txt")), dir.join("b.txt"));
        assert_eq!(unique_path(&dir, Path::new("a.txt")), dir.join("a (1).txt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
