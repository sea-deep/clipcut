use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserOption {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthState {
    pub current_source: String,
    pub custom_path: Option<String>,
    pub browsers: Vec<BrowserOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieConfig {
    pub source: String, // "auto", "zen", "chrome", "firefox", "brave", "edge", "safari", "custom", "none"
    pub custom_path: Option<String>,
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            source: "auto".to_string(),
            custom_path: None,
        }
    }
}

static COOKIE_CONFIG: Lazy<StdMutex<CookieConfig>> = Lazy::new(|| {
    let cfg = load_persisted_config().unwrap_or_default();
    StdMutex::new(cfg)
});

fn config_file_path() -> PathBuf {
    paths::config_directory().join("cookies.json")
}

fn load_persisted_config() -> Option<CookieConfig> {
    let path = config_file_path();
    if !path.exists() {
        return None;
    }
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str::<CookieConfig>(&data).ok()
}

fn save_persisted_config(cfg: &CookieConfig) {
    let path = config_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn get_cookie_source() -> (String, Option<String>) {
    let lock = COOKIE_CONFIG.lock().unwrap();
    (lock.source.clone(), lock.custom_path.clone())
}

pub fn set_cookie_source(source: &str, custom_path: Option<String>) {
    let mut lock = COOKIE_CONFIG.lock().unwrap();
    lock.source = source.to_string();
    lock.custom_path = custom_path;
    save_persisted_config(&lock);
}

/// Find Zen browser profile directory containing cookies.sqlite
pub fn find_zen_profile() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let mut candidates = Vec::new();

    #[cfg(target_os = "macos")]
    {
        candidates.push(home.join("Library/Application Support/zen/Profiles"));
    }
    #[cfg(target_os = "linux")]
    {
        candidates.push(home.join(".zen"));
        candidates.push(home.join(".var/app/app.zen_browser.zen/.zen"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = dirs::data_dir() {
            candidates.push(appdata.join("zen/Profiles"));
        }
    }

    for base in candidates {
        if !base.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&base) {
            let mut profiles: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.join("cookies.sqlite").exists())
                .collect();

            profiles.sort_by_key(|p| {
                p.join("cookies.sqlite")
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            });

            if let Some(newest) = profiles.pop() {
                return Some(newest);
            }
        }
    }
    None
}

/// Find standard Firefox profile directory
pub fn find_firefox_profile() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let mut candidates = Vec::new();

    #[cfg(target_os = "macos")]
    {
        candidates.push(home.join("Library/Application Support/Firefox/Profiles"));
    }
    #[cfg(target_os = "linux")]
    {
        candidates.push(home.join(".mozilla/firefox"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = dirs::data_dir() {
            candidates.push(appdata.join("Mozilla/Firefox/Profiles"));
        }
    }

    for base in candidates {
        if !base.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&base) {
            let mut profiles: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.join("cookies.sqlite").exists())
                .collect();

            profiles.sort_by_key(|p| {
                p.join("cookies.sqlite")
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            });

            if let Some(newest) = profiles.pop() {
                return Some(newest);
            }
        }
    }
    None
}

pub fn is_chrome_available() -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    #[cfg(target_os = "macos")]
    {
        home.join("Library/Application Support/Google/Chrome")
            .is_dir()
    }
    #[cfg(target_os = "linux")]
    {
        home.join(".config/google-chrome").is_dir()
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .map(|d| d.join("Google/Chrome/User Data").is_dir())
            .unwrap_or(false)
    }
}

pub fn is_brave_available() -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    #[cfg(target_os = "macos")]
    {
        home.join("Library/Application Support/BraveSoftware/Brave-Browser")
            .is_dir()
    }
    #[cfg(target_os = "linux")]
    {
        home.join(".config/BraveSoftware/Brave-Browser").is_dir()
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .map(|d| d.join("BraveSoftware/Brave-Browser/User Data").is_dir())
            .unwrap_or(false)
    }
}

pub fn is_edge_available() -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    #[cfg(target_os = "macos")]
    {
        home.join("Library/Application Support/Microsoft Edge")
            .is_dir()
    }
    #[cfg(target_os = "linux")]
    {
        home.join(".config/microsoft-edge").is_dir()
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .map(|d| d.join("Microsoft/Edge/User Data").is_dir())
            .unwrap_or(false)
    }
}

pub fn is_safari_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            home.join("Library/Cookies").exists()
        } else {
            false
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Returns a comprehensive list of supported browsers and their detection state on this machine.
pub fn get_available_browsers() -> Vec<BrowserOption> {
    let mut browsers = Vec::new();

    let zen_profile = find_zen_profile();
    browsers.push(BrowserOption {
        id: "zen".to_string(),
        name: "Zen Browser".to_string(),
        available: zen_profile.is_some(),
        detail: zen_profile.map(|p| p.to_string_lossy().to_string()),
    });

    let chrome_avail = is_chrome_available();
    browsers.push(BrowserOption {
        id: "chrome".to_string(),
        name: "Google Chrome".to_string(),
        available: chrome_avail,
        detail: None,
    });

    let firefox_profile = find_firefox_profile();
    browsers.push(BrowserOption {
        id: "firefox".to_string(),
        name: "Mozilla Firefox".to_string(),
        available: firefox_profile.is_some(),
        detail: firefox_profile.map(|p| p.to_string_lossy().to_string()),
    });

    let brave_avail = is_brave_available();
    browsers.push(BrowserOption {
        id: "brave".to_string(),
        name: "Brave Browser".to_string(),
        available: brave_avail,
        detail: None,
    });

    let edge_avail = is_edge_available();
    browsers.push(BrowserOption {
        id: "edge".to_string(),
        name: "Microsoft Edge".to_string(),
        available: edge_avail,
        detail: None,
    });

    let safari_avail = is_safari_available();
    if cfg!(target_os = "macos") {
        browsers.push(BrowserOption {
            id: "safari".to_string(),
            name: "Apple Safari".to_string(),
            available: safari_avail,
            detail: None,
        });
    }

    browsers
}

/// Retrieve the active yt-dlp arguments for cookies according to current user setting or auto-detection.
pub fn active_cookie_args() -> Vec<String> {
    let (source, custom_path) = get_cookie_source();
    resolve_cookie_args_for(&source, custom_path.as_deref())
}

/// Resolve cookie arguments for a specific source ID.
pub fn resolve_cookie_args_for(source: &str, custom_path: Option<&str>) -> Vec<String> {
    match source {
        "none" => Vec::new(),
        "custom" => {
            if let Some(path) = custom_path {
                if Path::new(path).exists() {
                    return vec!["--cookies".to_string(), path.to_string()];
                }
            }
            Vec::new()
        }
        "zen" => {
            if let Some(profile) = find_zen_profile() {
                vec![
                    "--cookies-from-browser".to_string(),
                    format!("firefox:{}", profile.to_string_lossy()),
                ]
            } else {
                Vec::new()
            }
        }
        "firefox" => {
            if let Some(profile) = find_firefox_profile() {
                vec![
                    "--cookies-from-browser".to_string(),
                    format!("firefox:{}", profile.to_string_lossy()),
                ]
            } else {
                vec!["--cookies-from-browser".to_string(), "firefox".to_string()]
            }
        }
        "chrome" => vec!["--cookies-from-browser".to_string(), "chrome".to_string()],
        "brave" => vec!["--cookies-from-browser".to_string(), "brave".to_string()],
        "edge" => vec!["--cookies-from-browser".to_string(), "edge".to_string()],
        "safari" => vec!["--cookies-from-browser".to_string(), "safari".to_string()],
        "auto" => {
            // Auto mode: pick the first available browser with profile on disk
            if let Some(profile) = find_zen_profile() {
                return vec![
                    "--cookies-from-browser".to_string(),
                    format!("firefox:{}", profile.to_string_lossy()),
                ];
            }
            if is_chrome_available() {
                return vec!["--cookies-from-browser".to_string(), "chrome".to_string()];
            }
            if let Some(profile) = find_firefox_profile() {
                return vec![
                    "--cookies-from-browser".to_string(),
                    format!("firefox:{}", profile.to_string_lossy()),
                ];
            }
            if is_brave_available() {
                return vec!["--cookies-from-browser".to_string(), "brave".to_string()];
            }
            if is_edge_available() {
                return vec!["--cookies-from-browser".to_string(), "edge".to_string()];
            }
            if is_safari_available() {
                return vec!["--cookies-from-browser".to_string(), "safari".to_string()];
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_cookie_args_none() {
        let args = resolve_cookie_args_for("none", None);
        assert!(args.is_empty());
    }

    #[test]
    fn test_resolve_cookie_args_custom() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path().to_string_lossy().to_string();
        let args = resolve_cookie_args_for("custom", Some(&path));
        assert_eq!(args, vec!["--cookies".to_string(), path]);
    }

    #[test]
    fn test_get_available_browsers_contains_standard() {
        let browsers = get_available_browsers();
        assert!(!browsers.is_empty());
        assert!(browsers.iter().any(|b| b.id == "zen"));
        assert!(browsers.iter().any(|b| b.id == "chrome"));
        assert!(browsers.iter().any(|b| b.id == "firefox"));
    }
}
