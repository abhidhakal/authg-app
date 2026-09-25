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
    Emitter, Manager,
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
    pub prevent_close: AtomicBool,
}

impl Default for MenuAppState {
    fn default() -> Self {
        Self {
            last_blur: Mutex::new(None),
            prevent_close: AtomicBool::new(false),
        }
    }
}

fn get_vault_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    // Dev builds never touch the real vault (or its keystore entry, see vault_key)
    Ok(base.join(if cfg!(debug_assertions) { "vault-dev.enc" } else { "vault.enc" }))
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

const KEYSTORE_ACCOUNT: &str = "vault-key";

/// Reads the vault key from the macOS Keychain. Ok(None) = no item yet.
#[cfg(target_os = "macos")]
fn keystore_get(service: &str) -> Result<Option<Vec<u8>>, String> {
    const ERR_NOT_FOUND: i32 = -25300; // errSecItemNotFound
    match security_framework::passwords::get_generic_password(service, KEYSTORE_ACCOUNT) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.code() == ERR_NOT_FOUND => Ok(None),
        Err(e) => Err(format!("Could not read vault key from Keychain: {}", e)),
    }
}

#[cfg(target_os = "macos")]
fn keystore_set(service: &str, key: &[u8]) -> Result<(), String> {
    security_framework::passwords::set_generic_password(service, KEYSTORE_ACCOUNT, key)
        .map_err(|e| format!("Could not save vault key to Keychain: {}", e))
}

/// Reads the vault key from Windows Credential Manager. Ok(None) = no item yet.
#[cfg(windows)]
fn keystore_get(service: &str) -> Result<Option<Vec<u8>>, String> {
    let entry = keyring::Entry::new(service, KEYSTORE_ACCOUNT).map_err(|e| e.to_string())?;
    match entry.get_secret() {
        Ok(bytes) => Ok(Some(bytes)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Could not read vault key from Credential Manager: {}", e)),
    }
}

#[cfg(windows)]
fn keystore_set(service: &str, key: &[u8]) -> Result<(), String> {
    keyring::Entry::new(service, KEYSTORE_ACCOUNT)
        .and_then(|e| e.set_secret(key))
        .map_err(|e| format!("Could not save vault key to Credential Manager: {}", e))
}

/// The vault's AES key, kept in the OS keystore. Created on first use, but never when an
/// AUG3 vault already exists: a fresh key could not open it, so that must surface as an error.
/// The keystore entry is named after the app identifier (+ ".dev" in debug builds), so dev and
/// screenshot builds never touch the real app's key. Read once per launch, then cached: every
/// keystore read can show an OS permission prompt, so it must not happen on every save.
#[cfg(any(target_os = "macos", windows))]
fn vault_key(app: &tauri::AppHandle, path: &PathBuf) -> Result<Option<[u8; 32]>, String> {
    static CACHE: Mutex<Option<[u8; 32]>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap();
    if let Some(key) = *cache {
        return Ok(Some(key));
    }
    let service = format!("{}{}", app.config().identifier, if cfg!(debug_assertions) { ".dev" } else { "" });
    let key = match keystore_get(&service)? {
        Some(bytes) => <[u8; 32]>::try_from(bytes.as_slice())
            .map_err(|_| "Stored vault key has the wrong length".to_string())?,
        None if vault::is_keychain_vault(path) => {
            return Err("Vault key is missing from the system keystore, so the vault can't be opened".to_string());
        }
        None => {
            use rand::RngCore;
            let mut key = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut key);
            keystore_set(&service, &key)?;
            key
        }
    };
    *cache = Some(key);
    Ok(Some(key))
}

// ponytail: Linux keeps the old PIN-derived format; Secret Service isn't always running there.
// Add a keystore once there's a way to test it on real distros.
#[cfg(not(any(target_os = "macos", windows)))]
fn vault_key(_app: &tauri::AppHandle, _path: &PathBuf) -> Result<Option<[u8; 32]>, String> {
    Ok(None)
}

// Vault commands are async so they run off the main thread: a keystore permission prompt
// blocks the calling thread, and on the main thread that froze the whole app.
static VAULT_IO: Mutex<()> = Mutex::new(());

#[tauri::command]
async fn save_accounts_vault(app: tauri::AppHandle, accounts: Vec<OtpAccount>) -> Result<(), String> {
    let _io = VAULT_IO.lock().unwrap();
    let path = get_vault_path(&app)?;
    match vault_key(&app, &path)? {
        Some(key) => vault::save_vault_v3(&path, &key, &accounts),
        None => vault::save_vault_to_path(&path, "", &accounts),
    }
}

#[tauri::command]
async fn load_accounts_vault(app: tauri::AppHandle) -> Result<Vec<OtpAccount>, String> {
    let _io = VAULT_IO.lock().unwrap();
    let path = get_vault_path(&app)?;
    let key = vault_key(&app, &path)?;
    vault::load_vault(&path, key.as_ref(), "")
}

#[tauri::command]
fn check_vault_exists(app: tauri::AppHandle) -> Result<bool, String> {
    let path = get_vault_path(&app)?;
    Ok(path.exists())
}

#[tauri::command]
fn hide_main_window(app: tauri::AppHandle) -> Result<(), String> {
    hide_popup(&app);
    Ok(())
}

/// Hide like a native menu bar popover: tell the page, and hand focus back to the previous app.
fn hide_popup(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        let _ = window.emit("popup-hidden", ());
    }
    #[cfg(target_os = "macos")]
    let _ = app.hide();
}

#[allow(unexpected_cfgs)]
fn open_or_focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Some(state) = app.try_state::<MenuAppState>() {
            *state.last_blur.lock().unwrap() = None;
        }
        if let Some(Ok(Some(rect))) = app.tray_by_id("main").map(|t| t.rect()) {
            position_under_tray(&window, rect);
        } else {
            #[cfg(not(target_os = "macos"))]
            let _ = window.center();
        }
        #[cfg(target_os = "macos")]
        let _ = app.show();
        if window.is_minimized().unwrap_or(false) {
            let _ = window.unminimize();
        }
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
        let _ = window.set_focus();

        #[cfg(target_os = "macos")]
        {
            use objc::runtime::Object;
            use objc::{class, msg_send, sel, sel_impl};
            unsafe {
                let ns_app_cls = class!(NSApplication);
                let ns_app: *mut Object = msg_send![ns_app_cls, sharedApplication];
                let _: () = msg_send![ns_app, activateIgnoringOtherApps: true];
            }
        }
    }
}

fn position_under_tray(window: &tauri::WebviewWindow, rect: tauri::Rect) {
    let icon_pos = rect.position.to_physical::<f64>(1.0);
    let icon_size = rect.size.to_physical::<f64>(1.0);
    if icon_size.width <= 0.0 {
        return; // tray not laid out yet (e.g. right at launch)
    }
    let Ok(win) = window.outer_size() else { return };
    let mut x = icon_pos.x + icon_size.width / 2.0 - win.width as f64 / 2.0;
    let y = icon_pos.y + icon_size.height + 4.0;
    // Keep the window on the icon's monitor when the icon sits near a screen edge.
    if let Ok(Some(m)) = window.monitor_from_point(icon_pos.x, icon_pos.y) {
        let left = m.position().x as f64;
        let right = left + m.size().width as f64 - win.width as f64;
        x = x.clamp(left, right.max(left));
    }
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// `AUTHG_SCREENSHOT=1`: popup opens centred and ignores click-away, for marketing screenshots
/// (a dev build launched from a terminal can't take focus, so it would hide instantly).
fn screenshot_mode() -> bool {
    std::env::var_os("AUTHG_SCREENSHOT").is_some()
}

fn toggle_popup(app: &tauri::AppHandle) {
    let visible = app
        .get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false));
    if visible {
        hide_popup(app);
    } else {
        open_or_focus_main_window(app);
    }
}

/// Follow the user across Spaces and appear over full-screen apps, like native menu bar popovers.
#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
fn set_popup_collection_behavior(window: &tauri::WebviewWindow) {
    use objc::runtime::Object;
    use objc::{msg_send, sel, sel_impl};
    const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    const FULL_SCREEN_AUXILIARY: usize = 1 << 8;
    if let Ok(ns_window) = window.ns_window() {
        unsafe {
            let _: () = msg_send![ns_window as *mut Object, setCollectionBehavior: CAN_JOIN_ALL_SPACES | FULL_SCREEN_AUXILIARY];
        }
    }
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    open_or_focus_main_window(&app);
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
        let _ = timeout_secs;
        return Err("Concealed pasteboard is only supported on macOS".to_string());
    }

    #[cfg(target_os = "macos")]
    if timeout_secs > 0 {
        let code_to_clear = code.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(timeout_secs));
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
        Err(e) => {
            let err_str = e.to_string();
            // If the remote release JSON is not found, 404s, or remote has no release for this target,
            // treat it cleanly as up-to-date rather than alarming the user with a parsing error.
            if err_str.contains("Could not fetch a valid release JSON")
                || err_str.contains("ReleaseNotFound")
                || err_str.contains("404")
            {
                Ok(UpdateInfo {
                    should_update: false,
                    current_version: app.package_info().version.to_string(),
                    version: None,
                    body: None,
                    date: None,
                })
            } else {
                Err(format!("Check for update failed: {}", err_str))
            }
        }
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
                        if !state.prevent_close.load(Ordering::SeqCst) && !screenshot_mode() {
                            hide_popup(app);
                        }
                    } else {
                        state.prevent_close.store(false, Ordering::SeqCst);
                    }
                }
            }
        })
        .setup(|app| {
            // Menu bar only: no Dock icon, no Cmd-Tab entry.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Build system tray menu & icon
            let toggle_item = MenuItem::with_id(app, "toggle", "Open / Hide AuthG", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit AuthG", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&toggle_item, &quit_item])?;

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
                .ok()
                .or_else(|| app.default_window_icon().cloned());

            if let Some(icon) = tray_icon {
                #[allow(unused_mut)]
                let mut builder = TrayIconBuilder::with_id("main")
                    .icon(icon)
                    .icon_as_template(true)
                    .tooltip("AuthG - Google Authenticator for Desktop")
                    .show_menu_on_left_click(false);

                // On macOS an attached status-item menu opens on *every* click (AppKit handles it
                // before we see the event), so there we pop it up ourselves on right-click instead.
                #[cfg(not(target_os = "macos"))]
                {
                    builder = builder.menu(&tray_menu);
                }

                let menu_for_tray = tray_menu.clone();
                let _ = builder
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "quit" => app.exit(0),
                        "toggle" => toggle_popup(app),
                        _ => {}
                    })
                    .on_tray_icon_event(move |tray, event| match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            let app = tray.app_handle();
                            // This same click may have just blurred (and hidden) the window; don't reopen it.
                            let just_hidden = app
                                .try_state::<MenuAppState>()
                                .and_then(|s| *s.last_blur.lock().unwrap())
                                .is_some_and(|t| t.elapsed() < Duration::from_millis(500));
                            if !just_hidden {
                                toggle_popup(app);
                            }
                        }
                        #[cfg(target_os = "macos")]
                        TrayIconEvent::Click {
                            button: MouseButton::Right,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            // No position = at the cursor in screen coords, so the (hidden) window's location doesn't matter.
                            if let Some(window) = tray.app_handle().get_webview_window("main") {
                                let _ = window.popup_menu(&menu_for_tray);
                            }
                        }
                        _ => {}
                    })
                    .build(app);
            }

            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                set_popup_collection_behavior(&window);
            }

            // Launched at login: stay in the menu bar. Launched by hand: show where it lives.
            if !std::env::args().any(|a| a == "--autostart") {
                open_or_focus_main_window(app.handle());
            }
            if screenshot_mode() {
                if let Some(window) = app.get_webview_window("main") {
                    // A terminal-launched build can't come to the front, so pin it above everything
                    let _ = window.set_always_on_top(true);
                    let _ = window.center();
                    let _ = window.show();
                }
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
            set_prevent_close_on_blur,
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
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // Dock icon click / relaunching from Finder while already running.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                open_or_focus_main_window(app);
            }
            let _ = (app, event);
        });
}
