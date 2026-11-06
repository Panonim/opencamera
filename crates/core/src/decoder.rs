//! H.264 decoder that prefers the platform hardware decoder and falls back to openh264 so video never stops.

use crate::process::I420;
use openh264::formats::YUVSource;

/// Consecutive hardware decode errors after which the stream moves to openh264.
#[cfg(windows)]
const MAX_HW_ERRORS: u32 = 5;

/// Tightly packed I420 planes owned by the decoder and lent out as the latest frame.
#[derive(Default)]
struct Planes {
    width: usize,
    height: usize,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

impl Planes {
    fn resize(&mut self, width: usize, height: usize) {
        let chroma = width.div_ceil(2) * height.div_ceil(2);
        (self.width, self.height) = (width, height);
        self.y.resize(width * height, 0);
        self.u.resize(chroma, 0);
        self.v.resize(chroma, 0);
    }

    fn i420(&self) -> I420<'_> {
        I420 { width: self.width, height: self.height, y: &self.y, u: &self.u, v: &self.v, y_stride: self.width, uv_stride: self.width.div_ceil(2) }
    }
}

fn copy_plane(dst: &mut [u8], src: &[u8], stride: usize, width: usize) {
    for (d, s) in dst.chunks_exact_mut(width).zip(src.chunks(stride)) {
        d.copy_from_slice(&s[..width]);
    }
}

/// Splits an NV12 picture with `pitch` bytes per row and chroma starting at `uv_offset` into I420 planes.
#[cfg_attr(not(windows), allow(dead_code))]
fn nv12_to_i420(src: &[u8], pitch: usize, uv_offset: usize, width: usize, height: usize, out: &mut Planes) {
    out.resize(width, height);
    copy_plane(&mut out.y, src, pitch, width);
    let cw = width.div_ceil(2);
    for ((u, v), row) in out.u.chunks_exact_mut(cw).zip(out.v.chunks_exact_mut(cw)).zip(src[uv_offset..].chunks(pitch)) {
        for ((u, v), &[cb, cr]) in u.iter_mut().zip(v.iter_mut()).zip(row.as_chunks::<2>().0) {
            (*u, *v) = (cb, cr);
        }
    }
}

pub struct VideoDecoder {
    #[cfg(windows)]
    hw: Option<mf::MfDecoder>,
    #[cfg(windows)]
    hw_errors: u32,
    soft: openh264::decoder::Decoder,
    planes: Planes,
}

impl VideoDecoder {
    pub fn new() -> Option<Self> {
        Some(Self {
            #[cfg(windows)]
            hw: unsafe { mf::MfDecoder::new() }.inspect_err(|e| tracing::warn!("hardware H.264 decoder unavailable: {e}")).ok(),
            #[cfg(windows)]
            hw_errors: 0,
            soft: openh264::decoder::Decoder::new().ok()?,
            planes: Planes::default(),
        })
    }

    pub fn backend(&self) -> &'static str {
        #[cfg(windows)]
        if self.hw.is_some() {
            return "Media Foundation (hardware)";
        }
        "openh264 (software)"
    }

    /// Decodes one Annex-B access unit and returns the picture it completed, if any.
    pub fn decode(&mut self, annexb: &[u8]) -> Result<Option<I420<'_>>, String> {
        Ok(self.decode_into_planes(annexb)?.then(|| self.planes.i420()))
    }

    fn decode_into_planes(&mut self, annexb: &[u8]) -> Result<bool, String> {
        #[cfg(windows)]
        if let Some(hw) = self.hw.as_mut() {
            let res = unsafe { hw.decode(annexb, &mut self.planes) }.map_err(|e| e.to_string());
            self.hw_errors = if res.is_ok() { 0 } else { self.hw_errors + 1 };
            if self.hw_errors >= MAX_HW_ERRORS {
                tracing::warn!("hardware H.264 decoder keeps failing, switching to openh264");
                self.hw = None;
            }
            return res;
        }
        let Some(yuv) = self.soft.decode(annexb).map_err(|e| e.to_string())? else { return Ok(false) };
        let ((w, h), (y_stride, uv_stride, _)) = (yuv.dimensions(), yuv.strides());
        self.planes.resize(w, h);
        copy_plane(&mut self.planes.y, yuv.y(), y_stride, w);
        copy_plane(&mut self.planes.u, yuv.u(), uv_stride, w.div_ceil(2));
        copy_plane(&mut self.planes.v, yuv.v(), uv_stride, w.div_ceil(2));
        Ok(true)
    }
}

/// Media Foundation H.264 decoder MFT driven through a D3D11 device so decoding runs on the GPU (DXVA).
#[cfg(windows)]
mod mf {
    use super::{nv12_to_i420, Planes};
    use std::mem::ManuallyDrop;
    use std::ptr::null_mut;
    use windows::core::{Interface, Result};
    use windows::Win32::Foundation::{E_NOTIMPL, E_UNEXPECTED, HMODULE};
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{D3D11CreateDevice, ID3D11Multithread, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION};
    use windows::Win32::Graphics::Dxgi::IDXGIAdapter;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

    /// 100 ns units per frame at 30 fps, only used to give samples increasing timestamps.
    const FRAME_TIME: i64 = 333_333;

    /// Shuts Media Foundation down after every COM object above it in `MfDecoder` is released.
    struct Runtime;

    impl Drop for Runtime {
        fn drop(&mut self) {
            let _ = unsafe { MFShutdown() };
        }
    }

    #[derive(Default)]
    struct Output {
        coded: (usize, usize),
        display: (usize, usize),
        provides_samples: bool,
        size: u32,
    }

    pub struct MfDecoder {
        mft: IMFTransform,
        _manager: IMFDXGIDeviceManager,
        out: Output,
        time: i64,
        _runtime: Runtime,
    }

    impl MfDecoder {
        pub unsafe fn new() -> Result<Self> {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_LITE)?;
            let runtime = Runtime;
            let mut device = None;
            D3D11CreateDevice(None::<&IDXGIAdapter>, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), D3D11_CREATE_DEVICE_VIDEO_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, None)?;
            let device = device.ok_or(E_UNEXPECTED)?;
            // The decoder and our readback touch the device from different threads.
            let _ = device.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
            let (mut token, mut manager) = (0, None);
            MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
            let manager = manager.ok_or(E_UNEXPECTED)?;
            manager.ResetDevice(&device, token)?;

            let mft: IMFTransform = CoCreateInstance(&CLSID_MSH264DecoderMFT, None, CLSCTX_INPROC_SERVER)?;
            let attrs = mft.GetAttributes()?;
            if attrs.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0) == 0 {
                return Err(E_NOTIMPL.into());
            }
            attrs.SetUINT32(&CODECAPI_AVLowLatencyMode, 1)?;
            mft.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager.as_raw() as usize)?;
            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            mft.SetInputType(0, &input, 0)?;
            let out = negotiate(&mft)?;
            mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            Ok(Self { mft, _manager: manager, out, time: 0, _runtime: runtime })
        }

        pub unsafe fn decode(&mut self, annexb: &[u8], planes: &mut Planes) -> Result<bool> {
            if annexb.is_empty() {
                return Ok(false);
            }
            let buf = MFCreateMemoryBuffer(annexb.len() as u32)?;
            let mut ptr = null_mut();
            buf.Lock(&mut ptr, None, None)?;
            std::ptr::copy_nonoverlapping(annexb.as_ptr(), ptr, annexb.len());
            buf.Unlock()?;
            buf.SetCurrentLength(annexb.len() as u32)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buf)?;
            sample.SetSampleTime(self.time)?;
            self.time += FRAME_TIME;
            let mut got = false;
            if let Err(e) = self.mft.ProcessInput(0, &sample, 0) {
                if e.code() != MF_E_NOTACCEPTING {
                    return Err(e);
                }
                got = self.drain(planes)?;
                self.mft.ProcessInput(0, &sample, 0)?;
            }
            Ok(self.drain(planes)? || got)
        }

        /// Pulls every ready picture, keeping the newest, and follows resolution changes.
        unsafe fn drain(&mut self, planes: &mut Planes) -> Result<bool> {
            let mut got = false;
            loop {
                let sample = if self.out.provides_samples {
                    None
                } else {
                    let s = MFCreateSample()?;
                    s.AddBuffer(&MFCreateMemoryBuffer(self.out.size)?)?;
                    Some(s)
                };
                let mut bufs = [MFT_OUTPUT_DATA_BUFFER { pSample: ManuallyDrop::new(sample), ..Default::default() }];
                let mut status = 0;
                let res = self.mft.ProcessOutput(0, &mut bufs, &mut status);
                // The caller owns whatever sample and events the MFT left in the buffer.
                let sample = ManuallyDrop::take(&mut bufs[0].pSample);
                drop(ManuallyDrop::take(&mut bufs[0].pEvents));
                match res {
                    Ok(()) => {
                        if let Some(s) = sample {
                            self.copy_frame(&s, planes)?;
                            got = true;
                        }
                    }
                    Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(got),
                    Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => self.out = negotiate(&self.mft)?,
                    Err(e) => return Err(e),
                }
            }
        }

        unsafe fn copy_frame(&self, sample: &IMFSample, planes: &mut Planes) -> Result<()> {
            let buf = sample.GetBufferByIndex(0)?;
            if let Ok(b2) = buf.cast::<IMF2DBuffer2>() {
                let (mut scan0, mut pitch, mut start, mut len) = (null_mut(), 0, null_mut(), 0);
                b2.Lock2DSize(MF2DBuffer_LockFlags_Read, &mut scan0, &mut pitch, &mut start, &mut len)?;
                let avail = (len as usize).saturating_sub(scan0.offset_from(start) as usize);
                let res = self.copy_nv12(std::slice::from_raw_parts(scan0, avail), pitch as usize, planes);
                b2.Unlock2D()?;
                res
            } else {
                let (mut ptr, mut len) = (null_mut(), 0);
                buf.Lock(&mut ptr, None, Some(&mut len))?;
                let res = self.copy_nv12(std::slice::from_raw_parts(ptr, len as usize), self.out.coded.0, planes);
                buf.Unlock()?;
                res
            }
        }

        fn copy_nv12(&self, data: &[u8], pitch: usize, planes: &mut Planes) -> Result<()> {
            let ((w, h), coded_h) = (self.out.display, self.out.coded.1);
            let uv_offset = pitch * coded_h;
            if pitch < w || h > coded_h || data.len() < uv_offset + pitch * h.div_ceil(2) {
                return Err(E_UNEXPECTED.into());
            }
            nv12_to_i420(data, pitch, uv_offset, w, h, planes);
            Ok(())
        }
    }

    /// Picks the NV12 output type and reads the coded size, visible size and buffer rules it implies.
    unsafe fn negotiate(mft: &IMFTransform) -> Result<Output> {
        let mut i = 0;
        let t = loop {
            let t = mft.GetOutputAvailableType(0, i)?;
            if t.GetGUID(&MF_MT_SUBTYPE)? == MFVideoFormat_NV12 {
                break t;
            }
            i += 1;
        };
        mft.SetOutputType(0, &t, 0)?;
        let size = t.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let coded = ((size >> 32) as usize, size as u32 as usize);
        let mut area = MFVideoArea::default();
        let blob = std::slice::from_raw_parts_mut(&mut area as *mut _ as *mut u8, size_of::<MFVideoArea>());
        // The visible picture is often smaller than the coded one, e.g. 1080 rows inside 1088.
        let display = match t.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, blob, None) {
            Ok(()) if area.Area.cx > 0 && area.Area.cy > 0 => (area.Area.cx as usize, area.Area.cy as usize),
            _ => coded,
        };
        let info = mft.GetOutputStreamInfo(0)?;
        let provides_samples = info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0) as u32 != 0;
        Ok(Output { coded, display, provides_samples, size: info.cbSize })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_fixture() {
        let stream = include_bytes!("../tests/fixtures/high.h264");
        let mut dec = VideoDecoder::new().unwrap();
        let mut frames = 0;
        for nal in openh264::nal_units(stream) {
            if let Ok(Some(f)) = dec.decode(nal) {
                assert_eq!((f.width, f.height), (128, 96));
                assert_eq!((f.y.len(), f.u.len(), f.v.len()), (128 * 96, 64 * 48, 64 * 48));
                assert_eq!((f.y_stride, f.uv_stride), (128, 64));
                frames += 1;
            }
        }
        assert!(frames >= 2, "decoded {frames} frames on {}", dec.backend());
    }

    #[test]
    fn deinterleaves_nv12() {
        // 4x2 picture with a 6 byte pitch and 2 rows of padding before the chroma plane.
        let mut src = vec![0u8; 6 * 4 + 6];
        src[..4].copy_from_slice(&[1, 2, 3, 4]);
        src[6..10].copy_from_slice(&[5, 6, 7, 8]);
        src[24..28].copy_from_slice(&[10, 20, 11, 21]);
        let mut out = Planes::default();
        nv12_to_i420(&src, 6, 24, 4, 2, &mut out);
        assert_eq!(out.y, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!((out.u.as_slice(), out.v.as_slice()), ([10, 11].as_slice(), [20, 21].as_slice()));
    }
}
