//! Sends video to a Linux virtual camera created by the v4l2loopback module.

use crate::{Error, Result, Status, VirtualCamera, DEVICE_NAME};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

const SETUP_HINT: &str = "Load the v4l2loopback kernel module: sudo modprobe v4l2loopback devices=1 exclusive_caps=1 card_label=\"OpenCamera Virtual Camera\"";

const V4L2_BUF_TYPE_VIDEO_OUTPUT: u32 = 2;
const V4L2_FIELD_NONE: u32 = 1;
const V4L2_COLORSPACE_SRGB: u32 = 8;
const V4L2_PIX_FMT_YUYV: u32 = u32::from_le_bytes(*b"YUYV");

#[repr(C)]
struct Capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

#[repr(C)]
#[derive(Default)]
struct PixFormat {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    bytesperline: u32,
    sizeimage: u32,
    colorspace: u32,
    priv_: u32,
    flags: u32,
    ycbcr_enc: u32,
    quantization: u32,
    xfer_func: u32,
}

const FORMAT_REST: usize = 200 - std::mem::size_of::<PixFormat>();

// Matches the Linux video format layout, a 200-byte block that starts 8 bytes in.
#[repr(C)]
struct Format {
    type_: u32,
    _pad: u32,
    pix: PixFormat,
    _rest: [u8; FORMAT_REST],
}

const fn ioc(dir: u64, nr: u64, size: usize) -> u64 {
    (dir << 30) | ((size as u64) << 16) | ((b'V' as u64) << 8) | nr
}
const VIDIOC_QUERYCAP: u64 = ioc(2, 0, std::mem::size_of::<Capability>());
const VIDIOC_S_FMT: u64 = ioc(3, 5, std::mem::size_of::<Format>());

fn cstr(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Lists the virtual camera devices, putting the one named after this app first.
fn loopback_devices() -> Vec<PathBuf> {
    let mut found: Vec<(bool, PathBuf)> = Vec::new();
    let Ok(entries) = std::fs::read_dir("/dev") else { return Vec::new() };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("video")) {
            continue;
        }
        let Ok(file) = OpenOptions::new().read(true).write(true).open(&path) else { continue };
        let mut cap: Capability = unsafe { std::mem::zeroed() };
        if unsafe { libc::ioctl(file.as_raw_fd(), VIDIOC_QUERYCAP as _, &mut cap) } != 0 {
            continue;
        }
        if cstr(&cap.driver) == "v4l2 loopback" {
            found.push((cstr(&cap.card) != DEVICE_NAME, path));
        }
    }
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

pub fn status() -> Status {
    if loopback_devices().is_empty() {
        Status::NeedsSetup(SETUP_HINT.into())
    } else {
        Status::Available
    }
}

pub fn request_setup() -> Result<()> {
    let label = format!("card_label={DEVICE_NAME}");
    let out = std::process::Command::new("pkexec")
        .args(["modprobe", "v4l2loopback", "devices=1", "exclusive_caps=1", &label])
        .output()
        .map_err(|e| Error::Os(format!("could not run pkexec: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::NeedsSetup(format!(
            "modprobe failed ({}). Install the v4l2loopback package for your distribution, then retry.",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

struct Loopback {
    path: PathBuf,
    file: Option<File>,
    size: (u32, u32),
    yuyv: Vec<u8>,
}

pub fn open(width: u32, height: u32) -> Result<Box<dyn VirtualCamera>> {
    let path = loopback_devices().into_iter().next().ok_or_else(|| Error::NeedsSetup(SETUP_HINT.into()))?;
    let file = open_device(&path, width, height)?;
    tracing::info!("v4l2loopback output on {}", path.display());
    Ok(Box::new(Loopback { path, file: Some(file), size: (width, height), yuyv: vec![0; (width * height * 2) as usize] }))
}

/// Opens the device for writing and sets its picture format.
fn open_device(path: &Path, w: u32, h: u32) -> Result<File> {
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| Error::Os(format!("{}: {e}", path.display())))?;
    let mut fmt = Format {
        type_: V4L2_BUF_TYPE_VIDEO_OUTPUT,
        _pad: 0,
        pix: PixFormat {
            width: w,
            height: h,
            pixelformat: V4L2_PIX_FMT_YUYV,
            field: V4L2_FIELD_NONE,
            bytesperline: w * 2,
            sizeimage: w * h * 2,
            colorspace: V4L2_COLORSPACE_SRGB,
            ..Default::default()
        },
        _rest: [0; FORMAT_REST],
    };
    if unsafe { libc::ioctl(file.as_raw_fd(), VIDIOC_S_FMT as _, &mut fmt) } != 0 {
        return Err(Error::Os(format!("VIDIOC_S_FMT failed: {}", std::io::Error::last_os_error())));
    }
    Ok(file)
}

impl VirtualCamera for Loopback {
    fn send(&mut self, nv12: &[u8], width: u32, height: u32) -> Result<()> {
        // The format can only change once the old writer is closed, so the device is reopened at the new size.
        if self.file.is_none() || self.size != (width, height) {
            self.file = None;
            self.file = Some(open_device(&self.path, width, height)?);
            self.size = (width, height);
            self.yuyv.resize((width * height * 2) as usize, 0);
            tracing::info!("v4l2loopback output resized to {width}x{height}");
        }
        nv12_to_yuyv(nv12, &mut self.yuyv, width as usize, height as usize);
        self.file.as_mut().unwrap().write_all(&self.yuyv).map_err(|e| Error::Os(e.to_string()))
    }
}

fn nv12_to_yuyv(nv12: &[u8], out: &mut [u8], w: usize, h: usize) {
    let (y_plane, uv_plane) = nv12.split_at(w * h);
    for row in 0..h {
        let y = &y_plane[row * w..][..w];
        let uv = &uv_plane[(row / 2) * w..][..w];
        let dst = &mut out[row * w * 2..][..w * 2];
        for (i, px) in dst.chunks_exact_mut(4).enumerate() {
            px.copy_from_slice(&[y[2 * i], uv[2 * i], y[2 * i + 1], uv[2 * i + 1]]);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn struct_sizes_match_kernel_abi() {
        assert_eq!(std::mem::size_of::<super::Capability>(), 104);
        assert_eq!(std::mem::size_of::<super::Format>(), 208);
        assert_eq!(super::VIDIOC_S_FMT, 0xc0d0_5605);
        assert_eq!(super::VIDIOC_QUERYCAP, 0x8068_5600);
    }

    #[test]
    fn yuyv_conversion() {
        // A tiny 2x2 picture with four brightness values and one shared color pair.
        let nv12 = [1, 2, 3, 4, 10, 20];
        let mut out = [0u8; 8];
        super::nv12_to_yuyv(&nv12, &mut out, 2, 2);
        assert_eq!(out, [1, 10, 2, 20, 3, 10, 4, 20]);
    }
}
