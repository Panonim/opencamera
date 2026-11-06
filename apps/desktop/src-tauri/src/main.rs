#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Handles the admin relaunch that installs or removes the virtual camera, then exits.
    #[cfg(windows)]
    {
        use opencamera_core::vcam;
        let has_flag = |flag: &str| std::env::args().any(|a| a == flag);
        let result = if has_flag(vcam::INSTALL_FLAG) {
            Some(opencamera_desktop::install_virtual_camera())
        } else if has_flag(vcam::UNINSTALL_FLAG) {
            Some(vcam::uninstall_component().map_err(|e| e.to_string()))
        } else {
            None
        };
        if let Some(r) = result {
            std::process::exit(if r.is_ok() { 0 } else { 1 });
        }
        if !opencamera_desktop::webview2::ensure() {
            std::process::exit(1);
        }
    }
    opencamera_desktop::run();
}
