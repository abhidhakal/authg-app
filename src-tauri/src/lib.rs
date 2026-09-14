pub mod migration;
pub mod totp;
pub mod vault;

use migration::{parse_migration_uri, OtpAccount};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use totp::{generate_totp, OtpCodeResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountWithCode {
    pub account: OtpAccount,
    pub code_info: OtpCodeResult,
}

pub struct MenuAppState {
    pub last_blur: Mutex<Option<Instant>>,
    pub close_on_blur: AtomicBool,
    pub prevent_close: AtomicBool,
}

impl Default for MenuAppState {
    fn default() -> Self {
        Self {
            last_blur: Mutex::new(None),
            close_on_blur: AtomicBool::new(true),
            prevent_close: AtomicBool::new(false),
        }
    }
}

fn get_vault_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    Ok(base.join("vault.enc"))
}

#[tauri::command]
fn parse_migration(uri: String) -> Result<Vec<OtpAccount>, String> {
    parse_migration_uri(&uri)
}

#[tauri::command]
fn get_code(
    secret: String,
    algorithm: Option<String>,
    digits: Option<u32>,
    period: Option<u64>,
) -> Result<OtpCodeResult, String> {
    generate_totp(&secret, algorithm.as_deref(), digits, period)
}

#[tauri::command]
fn get_all_codes(accounts: Vec<OtpAccount>) -> Result<Vec<AccountWithCode>, String> {
    let mut results = Vec::new();
    for acc in accounts {
        match generate_totp(
            &acc.secret,
            Some(&acc.algorithm),
            Some(acc.digits),
            Some(acc.period),
        ) {
            Ok(code_info) => {
                results.push(AccountWithCode {
                    account: acc,
                    code_info,
                });
            }
            Err(_e) => {
                // Return placeholder if invalid secret
                results.push(AccountWithCode {
                    account: acc,
                    code_info: OtpCodeResult {
                        code: "------".to_string(),
                        seconds_remaining: 0,
                        period: 30,
                    },
                });
            }
        }
    }
    Ok(results)
}

#[tauri::command]
fn save_accounts_vault(
    app: tauri::AppHandle,
    pin: String,
    accounts: Vec<OtpAccount>,
) -> Result<(), String> {
    let path = get_vault_path(&app)?;
    vault::save_vault_to_path(&path, &pin, &accounts)
}

#[tauri::command]
fn load_accounts_vault(app: tauri::AppHandle, pin: String) -> Result<Vec<OtpAccount>, String> {
    let path = get_vault_path(&app)?;
    vault::load_vault_from_path(&path, &pin)
}

#[tauri::command]
fn check_vault_exists(app: tauri::AppHandle) -> Result<bool, String> {
    let path = get_vault_path(&app)?;
    Ok(path.exists())
}

#[tauri::command]
fn hide_main_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        if let Some(state) = app.try_state::<MenuAppState>() {
            *state.last_blur.lock().unwrap() = None;
        }
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(())
}

#[tauri::command]
fn set_close_on_blur(app: tauri::AppHandle, enable: bool) -> Result<(), String> {
    if let Some(state) = app.try_state::<MenuAppState>() {
        state.close_on_blur.store(enable, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
fn set_prevent_close_on_blur(app: tauri::AppHandle, prevent: bool) -> Result<(), String> {
    if let Some(state) = app.try_state::<MenuAppState>() {
        state.prevent_close.store(prevent, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
fn set_show_in_dock(app: tauri::AppHandle, show: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let policy = if show {
            ActivationPolicy::Regular
        } else {
            ActivationPolicy::Accessory
        };
        let _ = app.set_activation_policy(policy);
    }
    let _ = app;
    let _ = show;
    Ok(())
}

#[tauri::command]
fn set_always_on_top(app: tauri::AppHandle, always_on_top: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_always_on_top(always_on_top);
    }
    Ok(())
}

#[tauri::command]
fn wipe_vault(app: tauri::AppHandle) -> Result<(), String> {
    let path = get_vault_path(&app)?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn read_image_base64(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let lower = path.to_lowercase();
    let mime = if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "image/png"
    };
    Ok(format!("data:{};base64,{}", mime, b64))
}

use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
fn set_open_at_login(app: tauri::AppHandle, enable: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    if enable {
        let _ = autolaunch.enable();
    } else {
        let _ = autolaunch.disable();
    }
    Ok(())
}

#[tauri::command]
fn is_open_at_login(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(MenuAppState::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(focused) = event {
                let app = window.app_handle();
                if let Some(state) = app.try_state::<MenuAppState>() {
                    if !focused {
                        *state.last_blur.lock().unwrap() = Some(Instant::now());
                        if state.close_on_blur.load(Ordering::SeqCst)
                            && !state.prevent_close.load(Ordering::SeqCst)
                        {
                            let _ = window.hide();
                        }
                    } else {
                        state.prevent_close.store(false, Ordering::SeqCst);
                    }
                }
            }
        })
        .setup(|app| {
            // Enable auto-launch at login by default if not set
            let autolaunch = app.autolaunch();
            if !autolaunch.is_enabled().unwrap_or(false) {
                let _ = autolaunch.enable();
            }

            // Build system tray icon for menu bar / system tray
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
                .ok()
                .or_else(|| app.default_window_icon().cloned());

            if let Some(icon) = tray_icon {
                let _ = TrayIconBuilder::new()
                    .icon(icon)
                    .icon_as_template(true)
                    .tooltip("AuthG - Google Authenticator for Desktop")
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let just_blurred = if let Some(state) = app.try_state::<MenuAppState>() {
                                    state
                                        .last_blur
                                        .lock()
                                        .unwrap()
                                        .map_or(false, |t| t.elapsed() < Duration::from_millis(350))
                                } else {
                                    false
                                };

                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else if !just_blurred {
                                    if let Some(state) = app.try_state::<MenuAppState>() {
                                        *state.last_blur.lock().unwrap() = None;
                                    }
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                    })
                    .build(app);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            parse_migration,
            get_code,
            get_all_codes,
            save_accounts_vault,
            load_accounts_vault,
            check_vault_exists,
            hide_main_window,
            show_main_window,
            set_close_on_blur,
            set_prevent_close_on_blur,
            set_show_in_dock,
            set_always_on_top,
            wipe_vault,
            read_image_base64,
            set_open_at_login,
            is_open_at_login,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
