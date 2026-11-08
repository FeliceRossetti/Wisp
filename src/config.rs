//! User settings, stored as TOML in the platform config directory.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Apply the bundled modern theme on top of WhatsApp Web.
    pub theme_enabled: bool,
    /// Accent color used by the theme (any CSS color).
    pub accent: String,
    /// Hide to the tray instead of quitting when the window is closed.
    pub close_to_tray: bool,
    /// Start hidden in the tray.
    pub start_hidden: bool,
    /// Show desktop notifications for new messages.
    pub notifications: bool,
    /// Include the message text in notifications (off = sender only).
    pub notification_preview: bool,
    /// Page zoom factor (1.0 = 100%).
    pub zoom: f64,
    /// Override the user agent sent to WhatsApp Web (empty = platform default).
    pub user_agent: String,
    pub window: WindowState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    pub width: f64,
    pub height: f64,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub maximized: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme_enabled: true,
            accent: "#06bcbc".into(),
            close_to_tray: true,
            start_hidden: false,
            notifications: true,
            notification_preview: true,
            zoom: 1.0,
            user_agent: String::new(),
            window: WindowState::default(),
        }
    }
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1180.0,
            height: 780.0,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

pub struct Paths {
    pub config_file: PathBuf,
    pub user_css: PathBuf,
    pub webview_data: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Self> {
        let dirs =
            ProjectDirs::from("", "", "Wisp").context("could not find a home directory for settings")?;
        let config_dir = dirs.config_dir().to_path_buf();
        let data_dir = dirs.data_local_dir().to_path_buf();
        std::fs::create_dir_all(&config_dir)?;
        std::fs::create_dir_all(&data_dir)?;
        Ok(Self {
            config_file: config_dir.join("config.toml"),
            user_css: config_dir.join("user.css"),
            webview_data: data_dir.join("webview"),
        })
    }
}

impl Config {
    /// Load settings; a missing or broken file falls back to defaults.
    pub fn load(paths: &Paths) -> Self {
        match std::fs::read_to_string(&paths.config_file) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!(
                    "config: {} is invalid, using defaults: {e}",
                    paths.config_file.display()
                );
                Self::default()
            }),
            Err(_) => {
                let cfg = Self::default();
                cfg.save(paths);
                cfg
            }
        }
    }

    pub fn save(&self, paths: &Paths) {
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&paths.config_file, text) {
                    eprintln!("config: could not save: {e}");
                }
            }
            Err(e) => eprintln!("config: could not serialize: {e}"),
        }
    }

    pub fn clamp_zoom(z: f64) -> f64 {
        (z * 20.0).round().clamp(10.0, 60.0) / 20.0
    }
}
