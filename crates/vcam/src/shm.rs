//! Shared memory layout the app uses to hand video frames to the Windows camera DLL.

use std::sync::atomic::{AtomicU64, Ordering};

/// Marker value spelling "OCAM" for 1920x1080 frames, the only size older camera DLLs understand.
pub const MAGIC: u32 = 0x4F43_414D;
/// Marker value spelling "OCAS" for frames of any other size, which older camera DLLs skip instead of misreading.
pub const MAGIC_SIZED: u32 = 0x4F43_4153;
/// Shared memory name read by the Windows 11 camera service.
pub const GLOBAL_NAME: &str = "Global\\OpenCameraVirtualCameraFrames";
/// Shared memory name read by the Windows 10 camera running inside each camera app.
pub const LOCAL_NAME: &str = "Local\\OpenCameraVirtualCameraFrames";
pub const HEADER_SIZE: usize = 64;
/// Room for the largest frame, mappings made by older versions can be smaller.
pub const FRAME_SIZE: usize = (crate::MAX_WIDTH * crate::MAX_HEIGHT * 3 / 2) as usize;
pub const TOTAL_SIZE: usize = HEADER_SIZE + FRAME_SIZE;

#[repr(C)]
pub struct Header {
    pub magic: u32,
    pub width: u32,
    pub height: u32,
    pub _pad: u32,
    /// Counter that is odd while a frame is being written, so readers can skip half-written frames.
    pub seq: AtomicU64,
    /// Time of the last frame in milliseconds, used to notice when the app stops sending.
    pub written_ms: AtomicU64,
}

fn nv12_len(width: u32, height: u32) -> usize {
    width as usize * height as usize * 3 / 2
}

/// Copies a frame into shared memory of `size` writable bytes at `base`, returning false when it does not fit.
#[allow(clippy::missing_safety_doc)]
pub unsafe fn write_frame(base: *mut u8, size: usize, nv12: &[u8], width: u32, height: u32, now_ms: u64) -> bool {
    let len = nv12_len(width, height);
    if nv12.len() < len || size < HEADER_SIZE + len {
        return false;
    }
    let header = &*(base as *const Header);
    let seq = header.seq.load(Ordering::Relaxed);
    header.seq.store(seq | 1, Ordering::Release);
    std::sync::atomic::fence(Ordering::Release);
    let h = base as *mut Header;
    (*h).magic = if (width, height) == (1920, 1080) { MAGIC } else { MAGIC_SIZED };
    (*h).width = width;
    (*h).height = height;
    std::ptr::copy_nonoverlapping(nv12.as_ptr(), base.add(HEADER_SIZE), len);
    header.written_ms.store(now_ms, Ordering::Relaxed);
    header.seq.store((seq | 1) + 1, Ordering::Release);
    true
}

/// Copies the newest whole frame from shared memory of `size` readable bytes at `base` into `out`, returning its time, width and height.
#[allow(clippy::missing_safety_doc)]
pub unsafe fn read_frame(base: *const u8, size: usize, out: &mut [u8]) -> Option<(u64, u32, u32)> {
    if size < HEADER_SIZE {
        return None;
    }
    let header = &*(base as *const Header);
    for _ in 0..3 {
        let before = header.seq.load(Ordering::Acquire);
        let (w, h) = (header.width, header.height);
        let len = nv12_len(w, h);
        let valid = matches!(header.magic, MAGIC | MAGIC_SIZED) && w > 0 && h > 0 && w % 2 == 0 && h % 2 == 0;
        if before == 0 || before & 1 == 1 || !valid || len > out.len() || HEADER_SIZE + len > size {
            std::thread::yield_now();
            continue;
        }
        std::ptr::copy_nonoverlapping(base.add(HEADER_SIZE), out.as_mut_ptr(), len);
        let written = header.written_ms.load(Ordering::Relaxed);
        std::sync::atomic::fence(Ordering::Acquire);
        if header.seq.load(Ordering::Relaxed) == before {
            return Some((written, w, h));
        }
    }
    None
}

#[cfg(windows)]
pub use mapping::Mapping;

#[cfg(windows)]
mod mapping {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL, INVALID_HANDLE_VALUE};
    use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::System::Memory::*;

    pub struct Mapping {
        handle: HANDLE,
        view: MEMORY_MAPPED_VIEW_ADDRESS,
        size: usize,
    }

    unsafe impl Send for Mapping {}

    impl Mapping {
        /// Opens the named shared memory, or creates it when this process is allowed to.
        pub fn open_or_create(name: &str) -> Option<Self> {
            let name = HSTRING::from(name);
            unsafe {
                let handle = OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, false, &name).or_else(|_| {
                    // Lets every app, including sandboxed ones, read and write the shared memory.
                    let mut sd = PSECURITY_DESCRIPTOR::default();
                    ConvertStringSecurityDescriptorToSecurityDescriptorW(
                        windows::core::w!("D:(A;;GA;;;WD)(A;;GA;;;AC)S:(ML;;NW;;;LW)"),
                        SDDL_REVISION_1,
                        &mut sd,
                        None,
                    )?;
                    let attrs = SECURITY_ATTRIBUTES {
                        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                        lpSecurityDescriptor: sd.0,
                        bInheritHandle: false.into(),
                    };
                    let h = CreateFileMappingW(INVALID_HANDLE_VALUE, Some(&attrs), PAGE_READWRITE, 0, super::TOTAL_SIZE as u32, &name);
                    let _ = LocalFree(Some(HLOCAL(sd.0)));
                    h
                });
                let handle = handle.ok()?;
                // Maps the whole region, which an older version may have created smaller than TOTAL_SIZE.
                let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, 0);
                let mut info = MEMORY_BASIC_INFORMATION::default();
                let queried = !view.Value.is_null() && VirtualQuery(Some(view.Value), &mut info, std::mem::size_of_val(&info)) != 0;
                if !queried {
                    if !view.Value.is_null() {
                        let _ = UnmapViewOfFile(view);
                    }
                    let _ = CloseHandle(handle);
                    return None;
                }
                Some(Self { handle, view, size: info.RegionSize })
            }
        }

        pub fn ptr(&self) -> *mut u8 {
            self.view.Value as *mut u8
        }

        /// Number of bytes the mapping holds.
        pub fn size(&self) -> usize {
            self.size
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            unsafe {
                let _ = UnmapViewOfFile(self.view);
                let _ = CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(size: usize) -> Vec<u64> {
        vec![0u64; size.div_ceil(8)]
    }

    #[test]
    fn roundtrip() {
        let mut region = region(TOTAL_SIZE);
        let base = region.as_mut_ptr() as *mut u8;
        let mut out = vec![0u8; FRAME_SIZE];
        assert_eq!(unsafe { read_frame(base, TOTAL_SIZE, &mut out) }, None);

        for (w, h, magic) in [(3840, 2160, MAGIC_SIZED), (1920, 1080, MAGIC), (1280, 720, MAGIC_SIZED)] {
            let len = (w * h * 3 / 2) as usize;
            let frame: Vec<u8> = (0..len).map(|i| ((i + w as usize) % 251) as u8).collect();
            assert!(unsafe { write_frame(base, TOTAL_SIZE, &frame, w, h, 42) });
            assert_eq!(unsafe { (*(base as *const Header)).magic }, magic);
            assert_eq!(unsafe { read_frame(base, TOTAL_SIZE, &mut out) }, Some((42, w, h)));
            assert_eq!(out[..len], frame[..]);
        }
    }

    #[test]
    fn small_mapping_fails_soft() {
        // A mapping made by an older version only fits a 1080p frame.
        let size = HEADER_SIZE + 1920 * 1080 * 3 / 2;
        let mut region = region(size);
        let base = region.as_mut_ptr() as *mut u8;
        assert!(!unsafe { write_frame(base, size, &vec![0; FRAME_SIZE], 3840, 2160, 1) });
        assert!(unsafe { write_frame(base, size, &vec![7; size - HEADER_SIZE], 1920, 1080, 1) });

        // A reader with a small buffer or mapping skips frames that do not fit.
        let mut out = vec![0u8; 1280 * 720 * 3 / 2];
        assert_eq!(unsafe { read_frame(base, size, &mut out) }, None);
        let mut out = vec![0u8; FRAME_SIZE];
        assert_eq!(unsafe { read_frame(base, size - 1, &mut out) }, None);
        assert_eq!(unsafe { read_frame(base, size, &mut out) }, Some((1, 1920, 1080)));
    }
}
