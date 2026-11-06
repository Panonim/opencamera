//! Sends video to the system virtual camera on each platform through one shared interface.

#[cfg(target_os = "linux")]
mod linux;
pub mod shm;
#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    clsid_string, has_mf_virtual_camera, install_component, uninstall_component, CLSID, CLSID_DSHOW, INSTALL_FLAG, UNINSTALL_FLAG,
};

pub const DEVICE_NAME: &str = "OpenCamera Virtual Camera";

/// Largest picture size the virtual camera can output.
pub const MAX_WIDTH: u32 = 3840;
pub const MAX_HEIGHT: u32 = 2160;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    NeedsSetup(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("virtual camera error: {0}")]
    Os(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Available,
    NeedsSetup(String),
    Unsupported(String),
}

/// A system camera device that receives NV12 video frames of the given size.
pub trait VirtualCamera: Send {
    fn send(&mut self, nv12: &[u8], width: u32, height: u32) -> Result<()>;
}

pub fn status() -> Status {
    #[cfg(windows)]
    return windows::status();
    #[cfg(target_os = "linux")]
    return linux::status();
    #[cfg(not(any(windows, target_os = "linux")))]
    Status::Unsupported(unsupported_reason())
}

/// Opens the virtual camera, starting with the given picture size where the platform needs one up front.
#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
pub fn open(width: u32, height: u32) -> Result<Box<dyn VirtualCamera>> {
    #[cfg(windows)]
    return windows::open();
    #[cfg(target_os = "linux")]
    return linux::open(width, height);
    #[cfg(not(any(windows, target_os = "linux")))]
    Err(Error::Unsupported(unsupported_reason()))
}

/// Asks the user for admin approval to set up the virtual camera.
pub fn request_setup() -> Result<()> {
    #[cfg(windows)]
    return windows::request_setup();
    #[cfg(target_os = "linux")]
    return linux::request_setup();
    #[cfg(not(any(windows, target_os = "linux")))]
    Err(Error::Unsupported(unsupported_reason()))
}

/// Describes the virtual camera setup to help with troubleshooting.
pub fn diagnostics() -> String {
    #[cfg(windows)]
    return windows::diagnostics();
    #[cfg(not(windows))]
    format!("{:?}", status())
}

/// Removes the installed virtual camera, which only applies on Windows.
pub fn request_removal() -> Result<()> {
    #[cfg(windows)]
    return windows::request_removal();
    #[cfg(not(windows))]
    Err(Error::Unsupported("Nothing to remove on this platform.".into()))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn unsupported_reason() -> String {
    "The macOS Camera Extension is not included in this build.".into()
}
