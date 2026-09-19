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
use tauri_plugin_updater::UpdaterExt;
use totp::{generate_totp, OtpCodeResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub should_update: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub body: Option<String>,
    pub date: Option<String>,
}

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
    pin: Option<String>,
    accounts: Vec<OtpAccount>,
) -> Result<(), String> {
    let path = get_vault_path(&app)?;
    vault::save_vault_to_path(&path, pin.as_deref().unwrap_or(""), &accounts)
}

#[tauri::command]
fn load_accounts_vault(app: tauri::AppHandle, pin: Option<String>) -> Result<Vec<OtpAccount>, String> {
    let path = get_vault_path(&app)?;
    vault::load_vault_from_path(&path, pin.as_deref().unwrap_or(""))
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
    let p = std::path::Path::new(&path);
    if !p.exists() || !p.is_file() {
        return Err("Target path is not a valid file".to_string());
    }

    let lower = path.to_lowercase();
    let is_img = lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".bmp");
    if !is_img {
        return Err("File is not a supported image format (.png, .jpg, .webp, .bmp)".to_string());
    }

    let metadata = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if metadata.len() > 10 * 1024 * 1024 {
        return Err("Image file exceeds maximum allowable size (10MB)".to_string());
    }

    let bytes = std::fs::read(p).map_err(|e| format!("Failed to read file: {}", e))?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let mime = if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else {
        "image/png"
    };
    Ok(format!("data:{};base64,{}", mime, b64))
}

#[tauri::command]
#[allow(unexpected_cfgs)]
fn copy_concealed_totp(code: String, timeout_secs: u64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc::runtime::Object;
        use objc::{class, msg_send, sel, sel_impl};
        use std::ffi::CString;

        unsafe {
            let pasteboard_cls = class!(NSPasteboard);
            let pasteboard: *mut Object = msg_send![pasteboard_cls, generalPasteboard];
            let _: () = msg_send![pasteboard, clearContents];

            let ns_string_cls = class!(NSString);
            let c_text = CString::new(code.clone()).map_err(|e| e.to_string())?;
            let ns_text: *mut Object = msg_send![ns_string_cls, stringWithUTF8String: c_text.as_ptr()];

            let string_type: *mut Object = msg_send![ns_string_cls, stringWithUTF8String: b"public.utf8-plain-text\0".as_ptr()];
            let concealed_type: *mut Object = msg_send![ns_string_cls, stringWithUTF8String: b"org.nspasteboard.ConcealedType\0".as_ptr()];
            let auto_gen_type: *mut Object = msg_send![ns_string_cls, stringWithUTF8String: b"org.nspasteboard.AutoGeneratedType\0".as_ptr()];

            let ns_data_cls = class!(NSData);
            let empty_data: *mut Object = msg_send![ns_data_cls, data];

            let _: () = msg_send![pasteboard, setString: ns_text forType: string_type];
            let _: () = msg_send![pasteboard, setData: empty_data forType: concealed_type];
            let _: () = msg_send![pasteboard, setData: empty_data forType: auto_gen_type];
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = &code;
    }

    if timeout_secs > 0 {
        let code_to_clear = code.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(timeout_secs));
            #[cfg(target_os = "macos")]
            {
                use objc::runtime::Object;
                use objc::{class, msg_send, sel, sel_impl};
                use std::ffi::CStr;

                unsafe {
                    let pasteboard_cls = class!(NSPasteboard);
                    let pasteboard: *mut Object = msg_send![pasteboard_cls, generalPasteboard];
                    let ns_string_cls = class!(NSString);
                    let string_type: *mut Object = msg_send![ns_string_cls, stringWithUTF8String: b"public.utf8-plain-text\0".as_ptr()];
                    let current_ns_str: *mut Object = msg_send![pasteboard, stringForType: string_type];

                    if !current_ns_str.is_null() {
                        let c_str: *const std::os::raw::c_char = msg_send![current_ns_str, UTF8String];
                        if !c_str.is_null() {
                            if let Ok(current_val) = CStr::from_ptr(c_str).to_str() {
                                if current_val == code_to_clear {
                                    let _: () = msg_send![pasteboard, clearContents];
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    Ok(())
}

#[tauri::command]
#[allow(unexpected_cfgs)]
fn check_camera_permission() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use objc::runtime::Object;
        use objc::{class, msg_send, sel, sel_impl};

        unsafe {
            let av_class = class!(AVCaptureDevice);
            let ns_string_cls = class!(NSString);
            let media_type_video: *mut Object = msg_send![ns_string_cls, stringWithUTF8String: b"vide\0".as_ptr()];
            let status: isize = msg_send![av_class, authorizationStatusForMediaType: media_type_video];

            match status {
                3 => Ok("authorized".to_string()),
                2 => Ok("denied".to_string()),
                1 => Ok("restricted".to_string()),
                0 => Ok("not_determined".to_string()),
                _ => Ok("unknown".to_string()),
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok("authorized".to_string())
    }
}

use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use tauri::menu::{Menu, MenuItem};

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

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

#[tauri::command]
async fn check_app_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let updater = app.updater().map_err(|e| format!("Updater initialization failed: {}", e))?;
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateInfo {
            should_update: true,
            current_version: update.current_version.clone(),
            version: Some(update.version.clone()),
            body: update.body.clone(),
            date: update.date.as_ref().map(|d| d.to_string()),
        }),
        Ok(None) => Ok(UpdateInfo {
            should_update: false,
            current_version: app.package_info().version.to_string(),
            version: None,
            body: None,
            date: None,
        }),
        Err(e) => Err(format!("Check for update failed: {}", e)),
    }
}

#[tauri::command]
async fn install_app_update(app: tauri::AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| format!("Updater initialization failed: {}", e))?;
    if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
        update
            .download_and_install(|_chunk, _total| {}, || {})
            .await
            .map_err(|e| format!("Download/install failed: {}", e))?;
        app.restart();
    } else {
        Err("No update available to install".to_string())
    }
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
        .plugin(tauri_plugin_updater::Builder::new().build())
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

            // Build system tray menu & icon
            let toggle_item = MenuItem::with_id(app, "toggle", "Open / Hide AuthG", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit AuthG", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&toggle_item, &quit_item])?;

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
                .ok()
                .or_else(|| app.default_window_icon().cloned());

            if let Some(icon) = tray_icon {
                let _ = TrayIconBuilder::new()
                    .icon(icon)
                    .icon_as_template(true)
                    .tooltip("AuthG - Google Authenticator for Desktop")
                    .menu(&tray_menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| {
                        match event.id.as_ref() {
                            "quit" => {
                                app.exit(0);
                            }
                            "toggle" => {
                                if let Some(window) = app.get_webview_window("main") {
                                    if window.is_visible().unwrap_or(false) {
                                        let _ = window.hide();
                                    } else {
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                    }
                                }
                            }
                            _ => {}
                        }
                    })
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
            quit_app,
            copy_concealed_totp,
            check_camera_permission,
            check_app_update,
            install_app_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
