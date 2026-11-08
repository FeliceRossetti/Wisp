//! Wisp: a lightweight, themed desktop shell for WhatsApp Web.
//!
//! Not affiliated with WhatsApp or Meta. It loads the official web client in the
//! system webview (WebView2, WKWebView or WebKitGTK) and adds a native window,
//! tray icon, desktop notifications and a modern theme on top.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod activity;
mod config;
mod icon;
mod lock;
mod web;

use anyhow::{Context, Result};
use config::{Config, Paths};
use std::path::{Path, PathBuf};
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::{Window, WindowBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use web::{Ipc, ZoomDir};
use wry::{NewWindowResponse, PermissionKind, PermissionResponse, WebContext, WebView, WebViewBuilder};

const APP_NAME: &str = "Wisp";

#[derive(Debug)]
enum UserEvent {
    Ipc(Ipc),
    Title(String),
    Tray(TrayIconEvent),
    Menu(MenuEvent),
    /// Result of a Windows Hello / Touch ID prompt.
    Auth(lock::SystemAuth),
}

/// Ids of the tray menu entries, so menu events can be matched.
struct TrayMenu {
    show: MenuItem,
    theme: CheckMenuItem,
    privacy: CheckMenuItem,
    notifications: CheckMenuItem,
    app_lock: CheckMenuItem,
    lock_now: MenuItem,
    reload: MenuItem,
    quit: MenuItem,
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
    let user_css = std::fs::read_to_string(&paths.user_css).unwrap_or_default();
    let system_unlock = lock::system_method();
    if cfg.app_lock && cfg.pin.is_none() && system_unlock.is_none() {
        // Nothing could unlock it; don't lock the user out of their own app.
        eprintln!("app lock: no PIN and no system unlock available; turning app lock off");
        cfg.app_lock = false;
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
    let window = wb.build(&event_loop).context("could not create the window")?;

    #[cfg(target_os = "macos")]
    let _app_menu = macos_app_menu();

    // ---- Webview ---------------------------------------------------------------
    let mut web_context = WebContext::new(Some(paths.webview_data.clone()));
    let webview = build_webview(
        &window,
        &mut web_context,
        &cfg,
        &user_css,
        system_unlock,
        event_loop.create_proxy(),
    )?;
    let _ = webview.zoom(cfg.zoom);

    if !cfg.start_hidden {
        window.set_visible(true);
        window.set_focus();
    }

    // ---- Event loop ------------------------------------------------------------
    let mut tray: Option<(TrayIcon, TrayMenu)> = None;
    let mut unread: u32 = 0;
    // Ask rather than assume: a window that opens behind others never gets the
    // focus events that would otherwise tell us.
    let mut focused = window.is_focused();
    let mut activity = activity::Activity::new(focused);
    // What the app has shown or hidden. tao applies visibility changes a moment later,
    // so asking the window right after a change can give a stale answer.
    let mut shown = !cfg.start_hidden;
    let mut minimized = false;
    let mut lock_state = lock::LockState::new(cfg.app_lock, system_unlock);
    let proxy = event_loop.create_proxy();

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        // Keep these alive for as long as the loop runs.
        let _ = &web_context;

        let mut quit = false;
        match event {
            Event::NewEvents(StartCause::Init) => {
                // The tray must be created once the loop is running (GTK needs it on Linux).
                match build_tray(&cfg) {
                    Ok(t) => tray = Some(t),
                    Err(e) => {
                        eprintln!("tray: unavailable ({e:#}); closing the window will quit");
                        show_window(&window, &mut shown);
                    }
                }
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    if cfg.close_to_tray && tray.is_some() {
                        hide_window(&window, &mut shown);
                    } else {
                        quit = true;
                    }
                }
                WindowEvent::Focused(f) => {
                    focused = f;
                    activity.set_focused(f);
                }
                WindowEvent::Resized(size) => {
                    minimized = window.is_minimized();
                    if !window.is_maximized() && !window.is_minimized() && size.width > 0 {
                        let logical = size.to_logical::<f64>(window.scale_factor());
                        cfg.window.width = logical.width;
                        cfg.window.height = logical.height;
                    }
                    cfg.window.maximized = window.is_maximized();
                }
                WindowEvent::Moved(pos) if !window.is_maximized() && !window.is_minimized() => {
                    cfg.window.x = Some(pos.x);
                    cfg.window.y = Some(pos.y);
                }
                _ => {}
            },

            Event::Reopen { .. } => show_window(&window, &mut shown),

            Event::UserEvent(UserEvent::Title(title)) => {
                let n = web::unread_from_title(&title);
                if n != unread {
                    unread = n;
                    update_unread(&window, tray.as_ref().map(|t| &t.0), n);
                }
            }

            Event::UserEvent(UserEvent::Ipc(msg)) => match msg {
                Ipc::Notify { title, body } => {
                    if cfg.notifications && !(focused && window.is_visible()) {
                        let (title, body) = notification_content(&cfg, lock_state.locked, title, body);
                        notify(title, body);
                    }
                }
                Ipc::Quit => quit = true,
                Ipc::Hide => {
                    if tray.is_some() {
                        hide_window(&window, &mut shown);
                    } else {
                        window.set_minimized(true);
                    }
                }
                Ipc::Theme { on } => {
                    cfg.theme_enabled = on;
                    if let Some((_, m)) = &tray {
                        m.theme.set_checked(on);
                    }
                    cfg.save(&paths);
                }
                Ipc::Privacy { on } => {
                    cfg.privacy_blur = on;
                    if let Some((_, m)) = &tray {
                        m.privacy.set_checked(on);
                    }
                    cfg.save(&paths);
                }
                Ipc::Ready => push_lock(&webview, &lock_state),
                Ipc::Lock => {
                    if cfg.app_lock {
                        lock_state.lock();
                        push_lock(&webview, &lock_state);
                    }
                }
                Ipc::Unlock => {
                    if lock_state.start_system_auth() {
                        let p = proxy.clone();
                        lock::request_system_auth(&window, move |r| {
                            let _ = p.send_event(UserEvent::Auth(r));
                        });
                        push_lock(&webview, &lock_state);
                    }
                }
                Ipc::Pin { pin } => {
                    lock_state.try_pin(cfg.pin.as_ref(), &pin, std::time::Instant::now());
                    push_lock(&webview, &lock_state);
                }
                Ipc::SetPin { pin } => {
                    // Only accepted while the "choose a PIN" screen is open.
                    if lock_state.setup && !lock_state.locked && lock::valid_pin(&pin) {
                        cfg.pin = Some(lock::PinHash::new(&pin));
                        cfg.app_lock = true;
                        lock_state.end_setup();
                        cfg.save(&paths);
                        push_lock(&webview, &lock_state);
                    }
                }
                Ipc::CancelSetup => {
                    lock_state.end_setup();
                    push_lock(&webview, &lock_state);
                }
                Ipc::Zoom { dir } => {
                    cfg.zoom = match dir {
                        ZoomDir::In => Config::clamp_zoom(cfg.zoom + 0.1),
                        ZoomDir::Out => Config::clamp_zoom(cfg.zoom - 0.1),
                        ZoomDir::Reset => 1.0,
                    };
                    let _ = webview.zoom(cfg.zoom);
                    cfg.save(&paths);
                }
            },

            // Clicking the tray icon takes focus from the window, so a click always shows it.
            Event::UserEvent(UserEvent::Tray(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            })) => show_window(&window, &mut shown),

            Event::UserEvent(UserEvent::Auth(result)) => {
                lock_state.finish_system_auth(result);
                push_lock(&webview, &lock_state);
            }

            Event::UserEvent(UserEvent::Menu(ev)) => {
                if let Some((_, m)) = &tray {
                    let id = ev.id();
                    if id == m.show.id() {
                        show_window(&window, &mut shown);
                    } else if id == m.reload.id() {
                        let _ = webview.reload();
                    } else if id == m.theme.id() {
                        cfg.theme_enabled = m.theme.is_checked();
                        let _ = webview.evaluate_script(&format!(
                            "window.__wisp && window.__wisp.setTheme({})",
                            cfg.theme_enabled
                        ));
                        cfg.save(&paths);
                    } else if id == m.privacy.id() {
                        cfg.privacy_blur = m.privacy.is_checked();
                        let _ = webview.evaluate_script(&format!(
                            "window.__wisp && window.__wisp.setPrivacy({})",
                            cfg.privacy_blur
                        ));
                        cfg.save(&paths);
                    } else if id == m.app_lock.id() {
                        if lock_state.locked {
                            // Can't be changed from the tray while locked.
                        } else if cfg.app_lock {
                            cfg.app_lock = false;
                            cfg.pin = None;
                            cfg.save(&paths);
                        } else {
                            // Turned on once a PIN is chosen in the window.
                            lock_state.begin_setup();
                            show_window(&window, &mut shown);
                            push_lock(&webview, &lock_state);
                        }
                    } else if id == m.lock_now.id() {
                        if cfg.app_lock {
                            lock_state.lock();
                            push_lock(&webview, &lock_state);
                        }
                    } else if id == m.notifications.id() {
                        cfg.notifications = m.notifications.is_checked();
                        cfg.save(&paths);
                    } else if id == m.quit.id() {
                        quit = true;
                    }
                }
            }

            _ => {}
        }

        // Let the webview rest while the window is hidden, minimized or left unfocused.
        let in_use = shown && !minimized && focused;
        let mut wake = activity.update(&webview, shown && !minimized);

        // Lock after a while away, and keep the tray in step with the lock.
        let now = std::time::Instant::now();
        lock_state.set_away(!in_use, now);
        let (locked_now, lock_due) = lock_state.check_idle(cfg.app_lock, cfg.lock_after_minutes, now);
        if locked_now {
            push_lock(&webview, &lock_state);
        }
        wake = match (wake, lock_due) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        if let Some(at) = wake {
            *control_flow = ControlFlow::WaitUntil(at);
        }
        if let Some((_, m)) = &tray {
            m.app_lock.set_checked(cfg.app_lock);
            m.app_lock.set_enabled(!lock_state.locked);
            m.lock_now.set_enabled(cfg.app_lock && !lock_state.locked);
        }

        if quit {
            cfg.save(&paths);
            tray = None;
            *control_flow = ControlFlow::Exit;
        }
    });
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

fn build_webview(
    window: &Window,
    web_context: &mut WebContext,
    cfg: &Config,
    user_css: &str,
    system_unlock: Option<&str>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<WebView> {
    let ipc_proxy = proxy.clone();
    let title_proxy = proxy;

    let mut builder = WebViewBuilder::new_with_web_context(web_context)
        .with_url(web::HOME)
        .with_initialization_script(web::init_script(&web::PageConfig {
            theme: cfg.theme_enabled,
            privacy: cfg.privacy_blur,
            accent: &cfg.accent,
            user_css,
            locked: cfg.app_lock,
            lock_system: system_unlock.unwrap_or(""),
        }))
        .with_devtools(cfg!(debug_assertions))
        .with_clipboard(true)
        .with_autoplay(true)
        .with_ipc_handler(move |req| {
            if !web::is_trusted_origin(&req.uri().to_string()) {
                return;
            }
            if let Some(msg) = Ipc::parse(req.body()) {
                let _ = ipc_proxy.send_event(UserEvent::Ipc(msg));
            }
        })
        .with_document_title_changed_handler(move |title| {
            let _ = title_proxy.send_event(UserEvent::Title(title));
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

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let webview = builder.build(window)?;
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window
            .default_vbox()
            .context("no GTK container for the webview")?;
        builder.build_gtk(vbox)?
    };
    Ok(webview)
}

fn build_tray(cfg: &Config) -> Result<(TrayIcon, TrayMenu)> {
    let m = TrayMenu {
        show: MenuItem::new(format!("Show {APP_NAME}"), true, None),
        theme: CheckMenuItem::new("Modern theme", true, cfg.theme_enabled, None),
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

/// What a notification may show: nothing personal while privacy blur is on,
/// and no message text when previews are off.
fn notification_content(cfg: &Config, locked: bool, title: String, body: String) -> (String, String) {
    if cfg.privacy_blur || locked {
        (APP_NAME.to_string(), "New message".to_string())
    } else if cfg.notification_preview {
        (title, body)
    } else {
        (title, String::new())
    }
}

fn push_lock(webview: &WebView, state: &lock::LockState) {
    let _ = webview.evaluate_script(&state.script());
}

fn notify(title: String, body: String) {
    // Some platforms block while the notification is delivered; keep the UI responsive.
    std::thread::spawn(move || {
        let mut n = notify_rust::Notification::new();
        n.summary(&title).appname(APP_NAME);
        if !body.is_empty() {
            n.body(&body);
        }
        if let Err(e) = n.show() {
            eprintln!("notification failed: {e}");
        }
    });
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
        assert_eq!(notification_content(&cfg, false, t, b), msg());
        cfg.notification_preview = false;
        let (t, b) = msg();
        assert_eq!(
            notification_content(&cfg, false, t, b),
            ("Ann".into(), String::new())
        );
        let (t, b) = msg();
        assert_eq!(
            notification_content(&cfg, true, t, b),
            (APP_NAME.into(), "New message".into())
        );
        cfg.privacy_blur = true;
        let (t, b) = msg();
        assert_eq!(
            notification_content(&cfg, false, t, b),
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
