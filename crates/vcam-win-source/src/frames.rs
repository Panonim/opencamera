use opencamera_vcam::shm;
use windows::Win32::System::SystemInformation::GetTickCount64;

/// Frames older than this many milliseconds mean the app has stopped sending.
const STALE_MS: u64 = 2000;
/// Size of the plain idle picture, kept tiny since it scales up to any size unchanged.
const IDLE_SIZE: (usize, usize) = (64, 36);

pub struct FrameReader {
    name: &'static str,
    mapping: Option<shm::Mapping>,
    last_attempt: u64,
    frame: Vec<u8>,
    /// Width and height of the picture held in `frame`.
    size: (usize, usize),
}

impl FrameReader {
    pub fn new(name: &'static str) -> Self {
        let mut r = Self { name, mapping: shm::Mapping::open_or_create(name), last_attempt: unsafe { GetTickCount64() }, frame: vec![0; shm::FRAME_SIZE], size: IDLE_SIZE };
        r.fill_idle();
        r
    }

    /// Loads the newest frame from the app, or a plain dark frame if none is coming in.
    fn refresh(&mut self) {
        let now = unsafe { GetTickCount64() };
        if self.mapping.is_none() && now - self.last_attempt > 1000 {
            self.last_attempt = now;
            self.mapping = shm::Mapping::open_or_create(self.name);
        }
        let read = self.mapping.as_ref().and_then(|m| unsafe { shm::read_frame(m.ptr(), m.size(), &mut self.frame) });
        match read {
            Some((t, w, h)) if now.saturating_sub(t) < STALE_MS => self.size = (w as usize, h as usize),
            _ => self.fill_idle(),
        }
    }

    fn fill_idle(&mut self) {
        let (w, h) = IDLE_SIZE;
        self.size = IDLE_SIZE;
        self.frame[..w * h].fill(24);
        self.frame[w * h..w * h * 3 / 2].fill(128);
    }

    /// Writes the current frame at the requested size in NV12 format.
    pub fn fill_nv12(&mut self, dst: *mut u8, pitch: usize, w: usize, h: usize) {
        self.refresh();
        unsafe { scale_nv12(&self.frame, self.size, dst, pitch, w, h) };
    }

    /// Writes the current frame at the requested size in YUY2 format.
    pub fn fill_yuy2(&mut self, dst: *mut u8, stride: usize, w: usize, h: usize) {
        self.refresh();
        unsafe { scale_yuy2(&self.frame, self.size, dst, stride, w, h) };
    }
}

/// Resizes an NV12 frame of size `(sw, sh)` by copying the nearest pixel.
unsafe fn scale_nv12(src: &[u8], (sw, sh): (usize, usize), dst: *mut u8, pitch: usize, w: usize, h: usize) {
    let xs: Vec<usize> = (0..w).map(|x| x * sw / w).collect();
    for y in 0..h {
        let s = &src[(y * sh / h) * sw..];
        let d = std::slice::from_raw_parts_mut(dst.add(y * pitch), w);
        if w == sw {
            d.copy_from_slice(&s[..w]);
        } else {
            for (x, px) in d.iter_mut().enumerate() {
                *px = s[xs[x]];
            }
        }
    }
    let (suv, duv) = (&src[sw * sh..], dst.add(pitch * h));
    for y in 0..h / 2 {
        let s = &suv[(y * (sh / 2) / (h / 2)) * sw..];
        let d = std::slice::from_raw_parts_mut(duv.add(y * pitch), w);
        if w == sw {
            d.copy_from_slice(&s[..w]);
        } else {
            for x in 0..w / 2 {
                let sx = (x * (sw / 2) / (w / 2)) * 2;
                d[2 * x] = s[sx];
                d[2 * x + 1] = s[sx + 1];
            }
        }
    }
}

/// Resizes an NV12 frame of size `(sw, sh)` into YUY2 format by copying the nearest pixel.
unsafe fn scale_yuy2(src: &[u8], (sw, sh): (usize, usize), dst: *mut u8, stride: usize, w: usize, h: usize) {
    let xs: Vec<usize> = (0..w).map(|x| x * sw / w).collect();
    let uv_plane = &src[sw * sh..];
    for y in 0..h {
        let sy = y * sh / h;
        let luma = &src[sy * sw..];
        let uv = &uv_plane[(sy / 2) * sw..];
        let d = std::slice::from_raw_parts_mut(dst.add(y * stride), w * 2);
        for (x, px) in d.chunks_exact_mut(4).enumerate() {
            let (x0, x1) = (xs[2 * x], xs[2 * x + 1]);
            let c = (x0 / 2) * 2;
            px.copy_from_slice(&[luma[x0], uv[c], luma[x1], uv[c + 1]]);
        }
    }
}
