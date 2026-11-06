//! System tray icon with a menu to open the app, toggle launch at login and quit.

use crate::i18n::t;
use crate::{save_settings, AppState};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

const OPEN: &str = "open";
const LAUNCH: &str = "launch_at_login";
const QUIT: &str = "quit";

struct Tray {
    open: MenuItem<Wry>,
    launch: CheckMenuItem<Wry>,
    quit: MenuItem<Wry>,
}

pub fn init(app: &AppHandle, launch_at_login: bool, locale: &str) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, OPEN, t(locale, "tray.open", &[]), true, None::<&str>)?;
    let launch = CheckMenuItem::with_id(app, LAUNCH, t(locale, "tray.launchAtLogin", &[]), true, launch_at_login, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, t(locale, "tray.quit", &[]), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &launch, &quit])?;
    let mut tray = TrayIconBuilder::new().tooltip("OpenCamera").menu(&menu).on_menu_event(|app, event| match event.id.as_ref() {
        OPEN => show_window(app),
        LAUNCH => toggle_launch(app),
        QUIT => app.exit(0),
        _ => {}
    });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(Tray { open, launch, quit });
    Ok(())
}

/// Relabels the menu after the language setting changes.
pub fn set_locale(app: &AppHandle, locale: &str) {
    if let Some(tray) = app.try_state::<Tray>() {
        let _ = tray.open.set_text(t(locale, "tray.open", &[]));
        let _ = tray.launch.set_text(t(locale, "tray.launchAtLogin", &[]));
        let _ = tray.quit.set_text(t(locale, "tray.quit", &[]));
    }
}

/// Keeps the menu checkbox in line with the saved setting.
pub fn sync_launch_at_login(app: &AppHandle, enabled: bool) {
    if let Some(tray) = app.try_state::<Tray>() {
        let _ = tray.launch.set_checked(enabled);
    }
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn toggle_launch(app: &AppHandle) {
    let mut settings = app.state::<AppState>().settings.lock().unwrap().clone();
    settings.launch_at_login = !settings.launch_at_login;
    match save_settings(app, settings.clone()) {
        Ok(()) => {
            let _ = app.emit("settings-changed", settings);
        }
        Err(e) => {
            // The menu already flipped the checkbox, so put it back to the real value.
            let actual = app.state::<AppState>().settings.lock().unwrap().launch_at_login;
            sync_launch_at_login(app, actual);
            eprintln!("{e}");
        }
    }
}
