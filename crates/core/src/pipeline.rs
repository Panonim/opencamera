//! Background worker that decodes phone video, adjusts it, and sends it to the virtual camera and preview.

use crate::decoder::VideoDecoder;
use crate::process::{ProcessingSettings, Processor};
use opencamera_vcam::VirtualCamera;
use serde::Serialize;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PREVIEW_INTERVAL: Duration = Duration::from_millis(66);
const IDLE_INTERVAL: Duration = Duration::from_millis(500);
/// How long the last frame keeps being repeated after phone video stops, so brief Wi-Fi stalls do not flash the placeholder.
const STALL_GRACE: Duration = Duration::from_secs(3);

pub enum Msg {
    Frame(Vec<u8>),
    /// Starts fresh for a new phone stream so leftover video from the old one is not mixed in.
    Reset,
    Settings(ProcessingSettings),
    OpenVirtualCamera,
    CloseVirtualCamera,
    Shutdown,
}

#[derive(Default)]
pub struct Counters {
    pub decoded: AtomicU64,
    pub dropped: AtomicU64,
    pub decode_errors: AtomicU64,
    pub pending: AtomicUsize,
    pub width: AtomicU32,
    pub height: AtomicU32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VirtualCameraState {
    /// Camera status: starting, active, needsSetup, unsupported or error.
    pub status: &'static str,
    pub message: Option<String>,
}

pub trait Events: Send + Sync + 'static {
    fn preview(&self, jpeg: Vec<u8>);
    fn request_keyframe(&self);
    fn virtual_camera(&self, state: VirtualCameraState);
    /// Records a developer-mode log line, built only when that mode is on.
    fn debug(&self, msg: &dyn Fn() -> String);
}

pub fn spawn(settings: ProcessingSettings, counters: Arc<Counters>, events: Arc<dyn Events>) -> Sender<Msg> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("opencamera-pipeline".into())
        .spawn(move || run(rx, settings, counters, events))
        .expect("spawn pipeline thread");
    tx
}

fn open_camera(events: &dyn Events, (w, h): (usize, usize)) -> Option<Box<dyn VirtualCamera>> {
    use opencamera_vcam::Error;
    let (cam, state) = match opencamera_vcam::open(w as u32, h as u32) {
        Ok(cam) => (Some(cam), VirtualCameraState { status: "active", message: None }),
        Err(Error::NeedsSetup(m)) => (None, VirtualCameraState { status: "needsSetup", message: Some(m) }),
        Err(Error::Unsupported(m)) => (None, VirtualCameraState { status: "unsupported", message: Some(m) }),
        Err(e) => (None, VirtualCameraState { status: "error", message: Some(e.to_string()) }),
    };
    events.virtual_camera(state);
    cam
}

fn send(camera: &mut Option<Box<dyn VirtualCamera>>, nv12: &[u8], (w, h): (usize, usize), events: &dyn Events) {
    if let Some(cam) = camera.as_mut() {
        if let Err(e) = cam.send(nv12, w as u32, h as u32) {
            events.debug(&|| format!("virtual camera write failed: {e}"));
        }
    }
}

/// What the virtual camera gets while waiting for phone video.
#[derive(Debug, PartialEq)]
enum Idle {
    /// Video is flowing, so nothing extra is sent.
    Live,
    RepeatLast,
    Placeholder,
}

fn idle_action(since_frame: Option<Duration>) -> Idle {
    match since_frame {
        Some(d) if d <= IDLE_INTERVAL => Idle::Live,
        Some(d) if d <= STALL_GRACE => Idle::RepeatLast,
        _ => Idle::Placeholder,
    }
}

fn new_decoder(events: &dyn Events) -> Option<VideoDecoder> {
    let decoder = VideoDecoder::new();
    match &decoder {
        Some(d) => events.debug(&|| format!("H.264 decoder: {}", d.backend())),
        None => events.debug(&|| "H.264 decoder could not be created".into()),
    }
    decoder
}

fn run(rx: Receiver<Msg>, settings: ProcessingSettings, counters: Arc<Counters>, events: Arc<dyn Events>) {
    let mut decoder = new_decoder(&*events);
    let mut processor = Processor::new(settings);
    let mut camera = open_camera(&*events, processor.size());
    let mut last_frame: Option<Instant> = None;
    let mut last_idle = Instant::now() - IDLE_INTERVAL;
    let mut last_preview = Instant::now() - PREVIEW_INTERVAL;

    loop {
        match rx.recv_timeout(IDLE_INTERVAL) {
            Ok(Msg::Frame(data)) => {
                counters.pending.fetch_sub(1, Ordering::Relaxed);
                let Some(dec) = decoder.as_mut() else { continue };
                match dec.decode(&data) {
                    Ok(Some(frame)) => {
                        let (w, h) = (frame.width, frame.height);
                        if (w as u32, h as u32) != (counters.width.load(Ordering::Relaxed), counters.height.load(Ordering::Relaxed)) {
                            events.debug(&|| format!("decoding {w}x{h}"));
                        }
                        let size = processor.size();
                        let out = processor.process(&frame);
                        send(&mut camera, out, size, &*events);
                        if last_preview.elapsed() >= PREVIEW_INTERVAL {
                            last_preview = Instant::now();
                            events.preview(preview_jpeg(out, size));
                        }
                        counters.decoded.fetch_add(1, Ordering::Relaxed);
                        counters.width.store(w as u32, Ordering::Relaxed);
                        counters.height.store(h as u32, Ordering::Relaxed);
                        last_frame = Some(Instant::now());
                    }
                    Ok(None) => {}
                    Err(e) => {
                        events.debug(&|| format!("decode error on {} byte frame: {e}", data.len()));
                        counters.dropped.fetch_add(1, Ordering::Relaxed);
                        counters.decode_errors.fetch_add(1, Ordering::Relaxed);
                        events.request_keyframe();
                    }
                }
            }
            Ok(Msg::Reset) => {
                events.debug(&|| "decoder reset for new stream".into());
                decoder = new_decoder(&*events);
                events.request_keyframe();
            }
            Ok(Msg::Settings(s)) => processor.set_settings(s),
            Ok(Msg::OpenVirtualCamera) => {
                // Closes the old camera first so the system never shows two of them.
                drop(camera.take());
                camera = open_camera(&*events, processor.size());
            }
            Ok(Msg::CloseVirtualCamera) => drop(camera.take()),
            Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        // Keeps sending a picture while no phone video arrives so other apps do not freeze.
        if last_idle.elapsed() >= IDLE_INTERVAL {
            let size = processor.size();
            let frame = match idle_action(last_frame.map(|t| t.elapsed())) {
                Idle::Live => None,
                Idle::RepeatLast => Some(processor.output()),
                Idle::Placeholder => Some(processor.placeholder()),
            };
            if let Some(frame) = frame {
                last_idle = Instant::now();
                send(&mut camera, frame, size, &*events);
            }
        }
    }
}

/// Shrinks the current frame to about 640 pixels wide and saves it as a picture for the desktop preview.
pub fn preview_jpeg(nv12: &[u8], (w, h): (usize, usize)) -> Vec<u8> {
    let f = (w / 640).max(1);
    let (pw, ph) = (w / f, h / f);
    let full = |y: u8| (((y as i32 - 16) * 255 / 219).clamp(0, 255)) as u8;
    let chroma = |c: u8| (((c as i32 - 128) * 255 / 224 + 128).clamp(0, 255)) as u8;
    let mut ycc = Vec::with_capacity(pw * ph * 3);
    for py in 0..ph {
        let sy = py * f;
        let uv_row = &nv12[w * h + (sy / 2) * w..];
        for px in 0..pw {
            let sx = px * f;
            let ci = (sx / 2) * 2;
            ycc.extend_from_slice(&[full(nv12[sy * w + sx]), chroma(uv_row[ci]), chroma(uv_row[ci + 1])]);
        }
    }
    let mut jpeg = Vec::with_capacity(48 * 1024);
    let encoder = jpeg_encoder::Encoder::new(&mut jpeg, 72);
    if encoder.encode(&ycc, pw as u16, ph as u16, jpeg_encoder::ColorType::Ycbcr).is_err() {
        jpeg.clear();
    }
    jpeg
}

/// Tells whether a chunk of video holds a full picture that decoding can start from.
pub fn is_keyframe(annexb: &[u8]) -> bool {
    annexb.windows(4).any(|w| w[..3] == [0, 0, 1] && matches!(w[3] & 0x1f, 5 | 7))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_keyframes() {
        assert!(is_keyframe(&[0, 0, 0, 1, 0x67, 1, 2]));
        assert!(is_keyframe(&[0, 0, 1, 0x65, 9]));
        assert!(!is_keyframe(&[0, 0, 0, 1, 0x41, 9]));
    }

    #[test]
    fn preview_is_valid_jpeg() {
        for (w, h) in [(1280, 720), (1920, 1080), (3840, 2160)] {
            let jpeg = preview_jpeg(&vec![128u8; w * h * 3 / 2], (w, h));
            assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        }
    }

    #[test]
    fn repeats_last_frame_during_brief_stalls() {
        let ms = Duration::from_millis;
        assert_eq!(idle_action(Some(ms(100))), Idle::Live);
        assert_eq!(idle_action(Some(ms(600))), Idle::RepeatLast);
        assert_eq!(idle_action(Some(STALL_GRACE)), Idle::RepeatLast);
        assert_eq!(idle_action(Some(STALL_GRACE + ms(1))), Idle::Placeholder);
        assert_eq!(idle_action(None), Idle::Placeholder);
    }
}
