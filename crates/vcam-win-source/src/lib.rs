//! The virtual camera DLL that Windows and camera apps load to get video from the app.
#![cfg(windows)]
#![allow(non_snake_case)]

mod attributes;
mod dshow;
mod frames;
mod source;
mod stream;

use opencamera_vcam::{clsid_string, has_mf_virtual_camera, CLSID, CLSID_DSHOW, DEVICE_NAME};
use std::sync::atomic::{AtomicPtr, Ordering};
use windows::core::{Interface, GUID, HRESULT, HSTRING, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Media::MediaFoundation::MF_E_SHUTDOWN;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, IClassFactory, IClassFactory_Impl, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_core::{implement, Ref, BOOL};

static MODULE: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

#[no_mangle]
extern "system" fn DllMain(module: HMODULE, reason: u32, _: *mut core::ffi::c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        MODULE.store(module.0, Ordering::Relaxed);
    }
    TRUE
}

#[implement(IClassFactory)]
struct Factory {
    clsid: GUID,
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<windows_core::IUnknown>, riid: *const GUID, ppv: *mut *mut core::ffi::c_void) -> windows_core::Result<()> {
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let object: windows_core::IUnknown =
            if self.clsid == CLSID_DSHOW { dshow::create_filter()?.cast()? } else { source::create_activator(CLSID)?.cast()? };
        unsafe { object.query(riid, ppv).ok() }
    }

    fn LockServer(&self, _: BOOL) -> windows_core::Result<()> {
        Ok(())
    }
}

#[no_mangle]
unsafe extern "system" fn DllGetClassObject(clsid: *const GUID, riid: *const GUID, ppv: *mut *mut core::ffi::c_void) -> HRESULT {
    if ppv.is_null() {
        return E_POINTER;
    }
    *ppv = std::ptr::null_mut();
    if *clsid != CLSID && *clsid != CLSID_DSHOW {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = Factory { clsid: *clsid }.into();
    factory.query(riid, ppv)
}

#[no_mangle]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    // Never lets Windows unload the DLL, since video may still be streaming.
    S_FALSE
}

fn clsid_key(clsid: &GUID) -> HSTRING {
    HSTRING::from(format!("Software\\Classes\\CLSID\\{}", clsid_string(clsid)))
}

/// Turns text into the null-terminated wide-character bytes the registry stores.
fn wide_bytes(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect()
}

/// Creates a machine-wide registry key and writes the given text values into it.
unsafe fn write_key(path: &HSTRING, values: &[(PCWSTR, &[u8])]) -> windows_core::Result<()> {
    let mut key = HKEY::default();
    RegCreateKeyExW(HKEY_LOCAL_MACHINE, path, None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None).ok()?;
    let result = values.iter().try_for_each(|&(name, value)| RegSetValueExW(key, name, None, REG_SZ, Some(value)).ok());
    let _ = RegCloseKey(key);
    result
}

unsafe fn register_inproc(clsid: &GUID, friendly: &str) -> windows_core::Result<()> {
    let mut path = [0u16; 1024];
    let len = GetModuleFileNameW(Some(HMODULE(MODULE.load(Ordering::Relaxed))), &mut path) as usize;
    let module = std::slice::from_raw_parts(path.as_ptr() as *const u8, (len + 1) * 2);
    let root = clsid_key(clsid);
    write_key(&root, &[(PCWSTR::null(), wide_bytes(friendly).as_slice())])?;
    let server = HSTRING::from(format!("{root}\\InprocServer32"));
    write_key(&server, &[(PCWSTR::null(), module), (windows::core::w!("ThreadingModel"), wide_bytes("Both").as_slice())])
}

unsafe fn unregister_inproc(clsid: &GUID) {
    let _ = RegDeleteTreeW(HKEY_LOCAL_MACHINE, &clsid_key(clsid));
}

unsafe fn unregister_dshow() {
    dshow::unregister();
    unregister_inproc(&CLSID_DSHOW);
}

/// Runs the given steps with Windows component services turned on.
unsafe fn with_com<T>(f: impl FnOnce() -> T) -> T {
    let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let result = f();
    if com.is_ok() {
        CoUninitialize();
    }
    result
}

/// Registers the camera for all users, exposing only the Windows 10 or Windows 11 version so it never shows twice.
unsafe fn register() -> windows_core::Result<()> {
    register_inproc(&CLSID, DEVICE_NAME)?;
    if has_mf_virtual_camera() {
        unregister_dshow();
        return Ok(());
    }
    register_inproc(&CLSID_DSHOW, DEVICE_NAME)?;
    let registered = dshow::register(DEVICE_NAME);
    if registered.is_err() {
        // Undoes a partly finished Windows 10 registration.
        unregister_dshow();
    }
    registered
}

#[no_mangle]
unsafe extern "system" fn DllRegisterServer() -> HRESULT {
    with_com(|| register()).map_or_else(|e| e.code(), |_| S_OK)
}

#[no_mangle]
unsafe extern "system" fn DllUnregisterServer() -> HRESULT {
    with_com(|| {
        unregister_dshow();
        unregister_inproc(&CLSID);
    });
    S_OK
}

pub(crate) fn ks_not_found() -> windows_core::Error {
    ERROR_SET_NOT_FOUND.to_hresult().into()
}

pub(crate) fn shutdown_err() -> windows_core::Error {
    MF_E_SHUTDOWN.into()
}
