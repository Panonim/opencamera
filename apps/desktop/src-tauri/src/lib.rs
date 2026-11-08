//! Runs the desktop app: saves settings, shows engine updates on screen and handles button actions.

mod i18n;
mod tray;
#[cfg(windows)]
pub mod webview2;

use opencamera_core::pairing::Device;
use opencamera_core::process::ProcessingSettings;
use opencamera_core::protocol::{IceServer, PhoneSettingsPatch};
#[cfg(windows)]
use opencamera_core::vcam;
use opencamera_core::{net, Diagnostics, Engine, EngineConfig, EngineEvents, EngineState, NetworkConfig, PairingInfo, Phase, Stats, VERSION};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

pub const DEFAULT_PORT: u16 = 47823;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Color theme: "system", "light" or "dark".
    pub theme: String,
    /// "system" or a locale code like "en".
    pub language: String,
    pub auto_reconnect: bool,
    pub notifications: bool,
    pub launch_at_login: bool,
    /// Hides the window to the system tray on close instead of quitting.
    pub keep_running: bool,
    pub port: u16,
    /// Network address to listen on, empty picks the main network automatically.
    pub address: String,
    /// Optional relay server address, empty keeps the connection on the local network.
    pub ice_server: String,
    pub processing: ProcessingSettings,
    pub grid: bool,
    /// Saved camera presets, kept as opaque JSON owned by the frontend.
    pub presets: Vec<serde_json::Value>,
    /// True once the first-run setup has been finished or skipped.
    pub onboarded: bool,
    /// Records detailed logs and shows them live in the app.
    pub developer_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            language: "system".into(),
            auto_reconnect: true,
            notifications: true,
            launch_at_login: false,
            keep_running: false,
            port: DEFAULT_PORT,
            address: String::new(),
            ice_server: String::new(),
            processing: ProcessingSettings::default(),
            grid: false,
            presets: Vec::new(),
            onboarded: false,
            developer_mode: false,
        }
    }
}

impl Settings {
    fn network(&self) -> NetworkConfig {
        let ice_server = self.ice_server.trim();
        NetworkConfig {
            port: self.port,
            address: self.address.parse().ok(),
            ice_servers: if ice_server.is_empty() {
                vec![]
            } else {
                vec![IceServer { urls: vec![ice_server.into()], username: None, credential: None }]
            },
        }
    }
}

struct AppState {
    engine: Mutex<Option<Engine>>,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
}

impl AppState {
    fn with_engine<T>(&self, f: impl FnOnce(&Engine) -> T) -> Result<T, String> {
        self.engine.lock().unwrap().as_ref().map(f).ok_or_else(|| "Engine is not running".into())
    }
}

/// Passes engine updates to the screen, the live preview and system notifications.
struct Events {
    app: AppHandle,
    preview: Mutex<Option<Channel<InvokeResponseBody>>>,
    last_phase: Mutex<Phase>,
    low_battery: Mutex<bool>,
}

/// Tells whether the phone reports 20% battery or less while not charging.
fn phone_battery_low(phone: Option<&serde_json::Value>) -> bool {
    let power = phone.and_then(|p| p.get("power"));
    let level = power.and_then(|p| p.get("battery")).and_then(|b| b.as_f64());
    let charging = power.and_then(|p| p.get("charging")).and_then(|c| c.as_bool());
    level.is_some_and(|l| l <= 0.2) && charging == Some(false)
}

impl Events {
    /// Shows the notification whose title and body sit under `key` in the locale files.
    fn notify(&self, key: &str, vars: &[(&str, &str)]) {
        let state = self.app.state::<AppState>();
        let (enabled, language) = {
            let settings = state.settings.lock().unwrap();
            (settings.notifications, settings.language.clone())
        };
        let focused = self.app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
        if enabled && !focused {
            let locale = i18n::resolve(&language);
            let title = i18n::t(locale, &format!("{key}.title"), vars);
            let body = i18n::t(locale, &format!("{key}.body"), vars);
            let _ = self.app.notification().builder().title(title).body(body).show();
        }
    }

    fn phone_name(&self, state: &EngineState) -> String {
        let language = self.app.state::<AppState>().settings.lock().unwrap().language.clone();
        state.device.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| i18n::t(i18n::resolve(&language), "notify.yourPhone", &[]))
    }
}

impl EngineEvents for Events {
    fn state(&self, state: &EngineState) {
        let previous = std::mem::replace(&mut *self.last_phase.lock().unwrap(), state.phase);
        if previous != state.phase {
            match state.phase {
                Phase::Connected if previous != Phase::Reconnecting => self.notify("notify.connected", &[("name", &self.phone_name(state))]),
                Phase::Disconnected if state.error.is_some() => self.notify("notify.disconnected", &[]),
                _ => {}
            }
        }
        let low = phone_battery_low(state.phone.as_ref());
        let was_low = std::mem::replace(&mut *self.low_battery.lock().unwrap(), low);
        if low && !was_low {
            self.notify("notify.batteryLow", &[]);
        }
        let _ = self.app.emit("engine-state", state);
    }

    fn stats(&self, stats: &Stats) {
        let _ = self.app.emit("engine-stats", stats);
    }

    fn preview(&self, jpeg: Vec<u8>) {
        if let Some(ch) = self.preview.lock().unwrap().as_ref() {
            let _ = ch.send(InvokeResponseBody::Raw(jpeg));
        }
    }

    fn devices(&self, _devices: &[Device]) {}

    fn log(&self, line: &str) {
        let _ = self.app.emit("engine-log", line);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    state: EngineState,
    settings: Settings,
    version: &'static str,
    platform: &'static str,
    addresses: Vec<String>,
}

#[tauri::command]
fn snapshot(app: State<AppState>) -> Result<Snapshot, String> {
    let settings = app.settings.lock().unwrap().clone();
    app.with_engine(|e| Snapshot {
        state: e.state(),
        settings,
        version: VERSION,
        platform: std::env::consts::OS,
        addresses: net::lan_addresses().iter().map(|a| a.to_string()).collect(),
    })
}

#[tauri::command]
fn subscribe_preview(events: State<Arc<Events>>, channel: Channel<InvokeResponseBody>) {
    *events.preview.lock().unwrap() = Some(channel);
}

#[tauri::command]
fn start_pairing(app: State<AppState>) -> Result<PairingInfo, String> {
    app.with_engine(|e| e.start_pairing().map_err(|e| e.to_string()))?
}

#[tauri::command]
fn cancel_pairing(app: State<AppState>) -> Result<(), String> {
    app.with_engine(|e| e.cancel_pairing())
}

#[tauri::command]
fn disconnect(app: State<AppState>) -> Result<(), String> {
    app.with_engine(|e| e.disconnect())
}

#[tauri::command]
fn clear_error(app: State<AppState>) -> Result<(), String> {
    app.with_engine(|e| e.clear_error())
}

#[tauri::command]
fn set_phone_settings(app: State<AppState>, patch: PhoneSettingsPatch) -> Result<(), String> {
    app.with_engine(|e| e.set_phone_settings(patch))
}

#[tauri::command]
fn diagnostics(app: State<AppState>) -> Result<Diagnostics, String> {
    app.with_engine(|e| e.diagnostics())
}

#[tauri::command]
fn retry_virtual_camera(app: State<AppState>) -> Result<(), String> {
    app.with_engine(|e| e.retry_virtual_camera())
}

/// Runs an engine action in the background because it waits for the admin approval prompt.
async fn run_blocking(handle: AppHandle, action: impl FnOnce(&Engine) -> Result<(), String> + Send + 'static) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || handle.state::<AppState>().with_engine(action)?)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn setup_virtual_camera(handle: AppHandle) -> Result<(), String> {
    run_blocking(handle, |e| e.setup_virtual_camera()).await
}

#[tauri::command]
async fn remove_virtual_camera(handle: AppHandle) -> Result<(), String> {
    run_blocking(handle, |e| e.remove_virtual_camera()).await
}

#[tauri::command]
fn update_settings(handle: AppHandle, settings: Settings) -> Result<(), String> {
    save_settings(&handle, settings)
}

/// Forgets every paired phone and restores default settings, which brings back the first-run screen.
#[tauri::command]
fn reset_all(handle: AppHandle) -> Result<(), String> {
    let app = handle.state::<AppState>();
    app.with_engine(|e| e.devices().iter().for_each(|d| e.forget_device(&d.id)))?;
    save_settings(&handle, Settings::default())
}

/// Saves the settings to disk and applies the changes to the engine, startup entry and tray menu.
fn save_settings(handle: &AppHandle, settings: Settings) -> Result<(), String> {
    let app = handle.state::<AppState>();
    let previous = std::mem::replace(&mut *app.settings.lock().unwrap(), settings.clone());
    let json = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&app.settings_path, json).map_err(|e| format!("Could not save settings: {e}"))?;
    app.with_engine(|e| {
        if previous.processing != settings.processing {
            e.set_processing(settings.processing);
        }
        if previous.developer_mode != settings.developer_mode {
            e.set_developer(settings.developer_mode);
        }
        if (previous.port, &previous.address, &previous.ice_server) != (settings.port, &settings.address, &settings.ice_server) {
            e.set_network(settings.network());
        }
    })?;
    if previous.launch_at_login != settings.launch_at_login {
        let autolaunch = handle.autolaunch();
        let result = if settings.launch_at_login { autolaunch.enable() } else { autolaunch.disable() };
        result.map_err(|e| format!("Could not change startup setting: {e}"))?;
    }
    tray::sync_launch_at_login(handle, settings.launch_at_login);
    if previous.language != settings.language {
        tray::set_locale(handle, i18n::resolve(&settings.language));
    }
    Ok(())
}

/// Installs the camera DLLs bundled in this build and runs in the admin copy of the app.
#[cfg(windows)]
pub fn install_virtual_camera() -> Result<(), String> {
    #[cfg(vcam_dll)]
    let dll64: Option<&[u8]> = Some(include_bytes!("../resources/opencamera_vcam.dll"));
    #[cfg(not(vcam_dll))]
    let dll64: Option<&[u8]> = None;
    #[cfg(vcam_dll32)]
    let dll32: Option<&[u8]> = Some(include_bytes!("../resources/opencamera_vcam32.dll"));
    #[cfg(not(vcam_dll32))]
    let dll32: Option<&[u8]> = None;
    let dll64 = dll64.ok_or("This build does not include the virtual camera component.")?;
    vcam::install_component(dll64, dll32).map_err(|e| e.to_string())
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let keep_running = window.app_handle().state::<AppState>().settings.lock().unwrap().keep_running;
                if keep_running {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let settings_path = data_dir.join("settings.json");
            let settings: Settings =
                std::fs::read(&settings_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();

            let events = Arc::new(Events {
                app: app.handle().clone(),
                preview: Mutex::new(None),
                last_phase: Mutex::new(Phase::Idle),
                low_battery: Mutex::new(false),
            });
            app.manage(events.clone());
            let engine = Engine::start(
                EngineConfig {
                    data_dir,
                    network: settings.network(),
                    processing: settings.processing,
                    accept_reconnects: settings.auto_reconnect,
                    developer: settings.developer_mode,
                },
                events,
            )
            .map_err(|e| e.to_string())?;
            if let Some(window) = app.get_webview_window("main") {
                // On Windows the app draws its own title bar and keeps the shadow so the window can still be resized.
                #[cfg(windows)]
                {
                    let _ = window.set_decorations(false);
                    let _ = window.set_shadow(true);
                }
                // Matches the title bar to the saved theme, or follows the system theme.
                let _ = window.set_theme(match settings.theme.as_str() {
                    "dark" => Some(tauri::Theme::Dark),
                    "light" => Some(tauri::Theme::Light),
                    _ => None,
                });
            }
            let launch_at_login = settings.launch_at_login;
            let locale = i18n::resolve(&settings.language);
            app.manage(AppState { engine: Mutex::new(Some(engine)), settings: Mutex::new(settings), settings_path });
            tray::init(app.handle(), launch_at_login, locale)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            subscribe_preview,
            start_pairing,
            cancel_pairing,
            disconnect,
            clear_error,
            set_phone_settings,
            diagnostics,
            retry_virtual_camera,
            setup_virtual_camera,
            remove_virtual_camera,
            update_settings,
            reset_all,
        ])
        .build(tauri::generate_context!())
        .expect("error while building OpenCamera");

    app.run(|handle, event| {
        // Shuts the engine down on exit so the virtual camera is removed and the phone is told.
        if let RunEvent::Exit = event {
            if let Some(state) = handle.try_state::<AppState>() {
                drop(state.engine.lock().unwrap().take());
            }
        }
    });
}
