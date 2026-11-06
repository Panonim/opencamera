//! Installs the Windows virtual camera and sends video to it on Windows 10 and 11.

use crate::{shm, Error, Result, Status, VirtualCamera, DEVICE_NAME};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use windows::core::{GUID, HSTRING, PCSTR, PCWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, REG_ROUTINE_FLAGS, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6432KEY};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// ID of the Windows 11 camera source.
pub const CLSID: GUID = GUID::from_u128(0xad527b53_2560_4330_bfda_6d8197ec41de);
/// ID of the Windows 10 camera source.
pub const CLSID_DSHOW: GUID = GUID::from_u128(0x5f3b8c71_9d2e_4a6b_8c41_2e7d9a0b6f13);
pub const INSTALL_FLAG: &str = "--install-vcam";
pub const UNINSTALL_FLAG: &str = "--uninstall-vcam";

type CreateFn = unsafe extern "system" fn(
    MFVirtualCameraType,
    MFVirtualCameraLifetime,
    MFVirtualCameraAccess,
    PCWSTR,
    PCWSTR,
    *const GUID,
    u32,
    *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT;

/// Looks up the Windows 11 camera function at runtime so the app still starts on Windows 10.
fn create_fn() -> Option<CreateFn> {
    unsafe {
        let lib = LoadLibraryW(windows::core::w!("mfsensorgroup.dll")).ok()?;
        GetProcAddress(lib, PCSTR(c"MFCreateVirtualCamera".as_ptr() as _)).map(|f| std::mem::transmute(f))
    }
}

/// Returns true on Windows 11 or newer, where the built-in virtual camera feature is used.
pub fn has_mf_virtual_camera() -> bool {
    os_build() >= 22000 && create_fn().is_some()
}

fn mode_name(mf: bool) -> &'static str {
    if mf {
        "Media Foundation"
    } else {
        "DirectShow"
    }
}

/// ID of the camera source that should be registered on this Windows version.
fn active_clsid(mf: bool) -> &'static GUID {
    if mf {
        &CLSID
    } else {
        &CLSID_DSHOW
    }
}

fn reg_string(key: &str, value: Option<&str>, flags: REG_ROUTINE_FLAGS) -> Option<String> {
    let key = HSTRING::from(key);
    let value = value.map(HSTRING::from);
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let name = value.as_ref().map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr()));
    let rc = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, &key, name, RRF_RT_REG_SZ | flags, None, Some(buf.as_mut_ptr() as _), Some(&mut len)) };
    rc.is_ok().then(|| String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]))
}

fn os_build() -> u32 {
    reg_string("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion", Some("CurrentBuildNumber"), REG_ROUTINE_FLAGS(0))
        .and_then(|b| b.trim().parse().ok())
        .unwrap_or(0)
}

pub fn clsid_string(clsid: &GUID) -> String {
    format!("{{{clsid:?}}}")
}

/// Finds the DLL file registered for this ID in the 64-bit or 32-bit registry.
fn registered_dll(clsid: &GUID, wow64: bool) -> Option<PathBuf> {
    let flags = if wow64 { RRF_SUBKEY_WOW6432KEY } else { REG_ROUTINE_FLAGS(0) };
    reg_string(&format!("Software\\Classes\\CLSID\\{}\\InprocServer32", clsid_string(clsid)), None, flags).map(PathBuf::from)
}

fn install_dir() -> PathBuf {
    let base = std::env::var_os("ProgramW6432")
        .or_else(|| std::env::var_os("ProgramFiles"))
        .unwrap_or_else(|| "C:\\Program Files".into());
    PathBuf::from(base).join("OpenCamera")
}

/// Location of the setup log that the installer writes and the app shows for troubleshooting.
fn log_path() -> PathBuf {
    let base = std::env::var_os("ProgramData").unwrap_or_else(|| "C:\\ProgramData".into());
    PathBuf::from(base).join("OpenCamera").join("setup.log")
}

fn log(msg: &str) {
    let path = log_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "[{secs}] {msg}");
    }
}

fn log_tail(lines: usize) -> String {
    let text = std::fs::read_to_string(log_path()).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

pub fn status() -> Status {
    let mf = has_mf_virtual_camera();
    // Asks for a reinstall when an old Windows 10 camera is left over after upgrading to Windows 11.
    if mf && registered_dll(&CLSID_DSHOW, false).is_some() {
        return Status::NeedsSetup("Update the virtual camera component for Windows 11.".into());
    }
    // Treats the camera as installed when it is registered and its DLL file exists.
    match registered_dll(active_clsid(mf), false) {
        Some(path) if path.exists() => Status::Available,
        Some(path) => Status::NeedsSetup(format!("The virtual camera component is missing ({}). Reinstall it.", path.display())),
        None => Status::NeedsSetup("Install the virtual camera component. Windows will ask for administrator approval.".into()),
    }
}

/// Builds a readable summary of the camera setup for the troubleshooting screen.
pub fn diagnostics() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Windows build {}, mode: {}", os_build(), mode_name(has_mf_virtual_camera()));
    for (label, clsid, wow64) in [("MF source", &CLSID, false), ("DirectShow 64-bit", &CLSID_DSHOW, false), ("DirectShow 32-bit", &CLSID_DSHOW, true)] {
        let state = match registered_dll(clsid, wow64) {
            Some(p) => format!("{} ({})", p.display(), if p.exists() { "present" } else { "missing" }),
            None => "not registered".into(),
        };
        let _ = writeln!(out, "{label}: {state}");
    }
    let tail = log_tail(8);
    if !tail.is_empty() {
        let _ = write!(out, "Last setup log:\n{tail}");
    }
    out
}

pub fn request_setup() -> Result<()> {
    run_elevated(INSTALL_FLAG)
}

pub fn request_removal() -> Result<()> {
    run_elevated(UNINSTALL_FLAG)
}

/// Restarts this app with admin rights to run one setup step and waits for it to finish.
fn run_elevated(flag: &str) -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| Error::Os(e.to_string()))?;
    let exe = HSTRING::from(exe.as_os_str());
    let args = HSTRING::from(flag);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: windows::core::w!("runas"),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(args.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        if let Err(e) = ShellExecuteExW(&mut info) {
            return Err(Error::NeedsSetup(format!("Administrator approval was declined or failed ({e}).")));
        }
        // Copies the process handle out first because this structure is tightly packed on 32-bit Windows.
        let process = info.hProcess;
        if process.is_invalid() {
            return Err(Error::Os("could not track the setup process".into()));
        }
        WaitForSingleObject(process, INFINITE);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(process, &mut code);
        let _ = CloseHandle(process);
        if code != 0 {
            return Err(Error::Os(format!("Setup failed (exit code {code}).\n{}", log_tail(6))));
        }
    }
    Ok(())
}

fn system_dir(wow64: bool) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join(if wow64 { "SysWOW64" } else { "System32" })
}

/// Registers or unregisters a DLL using the Windows regsvr32 tool that matches its 32 or 64-bit build.
fn regsvr32(dll: &Path, wow64: bool, unregister: bool) -> Result<()> {
    let mut cmd = std::process::Command::new(system_dir(wow64).join("regsvr32.exe"));
    cmd.arg("/s");
    if unregister {
        cmd.arg("/u");
    }
    let status = cmd.arg(dll).status().map_err(|e| Error::Os(format!("regsvr32: {e}")))?;
    let reason = match status.code() {
        Some(0) => return Ok(()),
        Some(3) => "the DLL could not be loaded",
        Some(4) => "the DLL has no registration entry point",
        Some(5) => "the DLL's registration failed",
        _ => "unknown error",
    };
    Err(Error::Os(format!("regsvr32 {}{}: {reason} ({status})", if unregister { "/u " } else { "" }, dll.display())))
}

/// Deletes a DLL now, or at the next restart if a camera app is still using it.
fn remove_dll(path: &Path) {
    if std::fs::remove_file(path).is_err() {
        let _ = unsafe { MoveFileExW(&HSTRING::from(path.as_os_str()), PCWSTR::null(), MOVEFILE_DELAY_UNTIL_REBOOT) };
    }
}

/// Makes a short fingerprint of the DLL so each new build gets its own file name.
fn content_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

fn write_dll(dir: &Path, prefix: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = dir.join(format!("{prefix}_{:016x}.dll", content_hash(bytes)));
    if !path.exists() {
        std::fs::write(&path, bytes).map_err(|e| Error::Os(format!("writing {}: {e}", path.display())))?;
    }
    Ok(path)
}

/// Copies the camera DLLs to Program Files, registers them for all users and logs the result.
pub fn install_component(dll64: &[u8], dll32: Option<&[u8]>) -> Result<()> {
    let result = install(dll64, dll32);
    match &result {
        Ok(()) => log("install finished"),
        Err(e) => log(&format!("install failed: {e}")),
    }
    result
}

fn install(dll64: &[u8], dll32: Option<&[u8]>) -> Result<()> {
    let mf = has_mf_virtual_camera();
    log(&format!("install started: Windows build {}, {}", os_build(), mode_name(mf)));
    let dir = install_dir();
    std::fs::create_dir_all(&dir).map_err(|e| Error::Os(format!("creating {}: {e}", dir.display())))?;
    let path64 = write_dll(&dir, "opencamera_vcam", dll64)?;
    regsvr32(&path64, false, false)?;
    log(&format!("registered {}", path64.display()));
    let mut keep = vec![path64.clone()];
    if let (false, Some(bytes)) = (mf, dll32) {
        let path32 = write_dll(&dir, "opencamera_vcam32", bytes)?;
        regsvr32(&path32, true, false)?;
        log(&format!("registered {}", path32.display()));
        keep.push(path32);
    }
    // Checks that Windows now points to the newly installed DLL.
    let clsid = active_clsid(mf);
    if registered_dll(clsid, false).as_deref() != Some(path64.as_path()) {
        return Err(Error::Os(format!("registration did not take effect ({} not registered)", clsid_string(clsid))));
    }
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if !keep.contains(&e.path()) && e.file_name().to_string_lossy().starts_with("opencamera_vcam") {
                remove_dll(&e.path());
            }
        }
    }
    Ok(())
}

pub fn uninstall_component() -> Result<()> {
    log("uninstall started");
    for (clsid, wow64) in [(&CLSID_DSHOW, true), (&CLSID_DSHOW, false), (&CLSID, false)] {
        if let Some(path) = registered_dll(clsid, wow64) {
            if let Err(e) = regsvr32(&path, wow64, true) {
                log(&format!("{e}"));
            }
            remove_dll(&path);
        }
    }
    log("uninstall finished");
    Ok(())
}

pub fn open() -> Result<Box<dyn VirtualCamera>> {
    if let Status::NeedsSetup(msg) | Status::Unsupported(msg) = status() {
        return Err(Error::NeedsSetup(msg));
    }
    match create_fn().filter(|_| has_mf_virtual_camera()) {
        Some(create) => open_media_foundation(create),
        // On Windows 10 the camera runs inside each camera app, so it only needs frames in shared memory.
        None => {
            let mapping = shm::Mapping::open_or_create(shm::LOCAL_NAME)
                .ok_or_else(|| Error::Os("could not create the shared frame buffer".into()))?;
            tracing::info!("DirectShow virtual camera ready");
            Ok(Box::new(DirectShowCam { mapping }))
        }
    }
}

struct DirectShowCam {
    mapping: shm::Mapping,
}

impl VirtualCamera for DirectShowCam {
    fn send(&mut self, nv12: &[u8], width: u32, height: u32) -> Result<()> {
        write(&self.mapping, nv12, width, height)
    }
}

fn write(m: &shm::Mapping, nv12: &[u8], width: u32, height: u32) -> Result<()> {
    if unsafe { shm::write_frame(m.ptr(), m.size(), nv12, width, height, GetTickCount64()) } {
        Ok(())
    } else {
        Err(Error::Os(format!("{width}x{height} does not fit the shared frame buffer, restart camera apps or reinstall the virtual camera")))
    }
}

struct MediaFoundationCam {
    camera: IMFVirtualCamera,
    mapping: Option<shm::Mapping>,
    last_attempt: Instant,
}

// Safe to move between threads because the camera is only used again when it is closed.
unsafe impl Send for MediaFoundationCam {}

fn open_media_foundation(create: CreateFn) -> Result<Box<dyn VirtualCamera>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|e| Error::Os(e.to_string()))?;
        let name = HSTRING::from(DEVICE_NAME);
        let source = HSTRING::from(clsid_string(&CLSID));
        let mut raw = std::ptr::null_mut();
        create(
            MFVirtualCameraType_SoftwareCameraSource,
            MFVirtualCameraLifetime_Session,
            MFVirtualCameraAccess_CurrentUser,
            PCWSTR(name.as_ptr()),
            PCWSTR(source.as_ptr()),
            std::ptr::null(),
            0,
            &mut raw,
        )
        .ok()
        .map_err(|e| Error::Os(format!("MFCreateVirtualCamera: {e}")))?;
        let camera = <IMFVirtualCamera as windows::core::Interface>::from_raw(raw);
        camera.Start(None).map_err(|e| Error::Os(format!("virtual camera start: {e}")))?;
        tracing::info!("virtual camera registered");
        Ok(Box::new(MediaFoundationCam { camera, mapping: shm::Mapping::open_or_create(shm::GLOBAL_NAME), last_attempt: Instant::now() }))
    }
}

impl VirtualCamera for MediaFoundationCam {
    fn send(&mut self, nv12: &[u8], width: u32, height: u32) -> Result<()> {
        // Retries opening shared memory once a second until the Windows camera service creates it.
        if self.mapping.is_none() && self.last_attempt.elapsed() > Duration::from_secs(1) {
            self.last_attempt = Instant::now();
            self.mapping = shm::Mapping::open_or_create(shm::GLOBAL_NAME);
        }
        self.mapping.as_ref().map_or(Ok(()), |m| write(m, nv12, width, height))
    }
}

impl Drop for MediaFoundationCam {
    fn drop(&mut self) {
        // Removes the camera completely so it does not show up twice on the next launch.
        unsafe {
            let _ = self.camera.Remove();
        }
    }
}
