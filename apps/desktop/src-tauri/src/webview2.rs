//! Makes sure the WebView2 runtime that draws the app window is installed on Windows.

use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONWARNING, MB_YESNO, SW_SHOWNORMAL};

const DOWNLOAD_URL: PCWSTR = w!("https://go.microsoft.com/fwlink/p/?LinkId=2124703");

/// Checks the registry places where Microsoft records an installed WebView2 runtime.
fn installed() -> bool {
    let keys: [(HKEY, PCWSTR); 3] = [
        (HKEY_LOCAL_MACHINE, w!("SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}")),
        (HKEY_LOCAL_MACHINE, w!("SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}")),
        (HKEY_CURRENT_USER, w!("Software\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}")),
    ];
    keys.iter().any(|(root, key)| {
        let mut buf = [0u16; 64];
        let mut len = (buf.len() * 2) as u32;
        let ok = unsafe { RegGetValueW(*root, *key, w!("pv"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as _), Some(&mut len)) }.is_ok();
        let version = String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]);
        ok && !version.is_empty() && version != "0.0.0.0"
    })
}

/// Returns true if WebView2 is installed, otherwise offers the download page and returns false.
pub fn ensure() -> bool {
    if installed() {
        return true;
    }
    let answer = unsafe {
        MessageBoxW(
            None,
            w!("OpenCamera needs the Microsoft Edge WebView2 Runtime, which is not installed on this PC.\n\nOpen the download page now?"),
            w!("OpenCamera"),
            MB_YESNO | MB_ICONWARNING,
        )
    };
    if answer == IDYES {
        unsafe { ShellExecuteW(None, w!("open"), DOWNLOAD_URL, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    }
    false
}
