use crate::frames::FrameReader;
use crate::{forward_attributes, shutdown_err};
use std::sync::Mutex;
use windows::core::{Interface, GUID};
use windows::Win32::Foundation::*;
use windows::Win32::Media::KernelStreaming::{IKsControl, IKsControl_Impl, KSIDENTIFIER, PINNAME_VIDEO_CAPTURE};
use windows::Win32::Media::MediaFoundation::*;
use windows_core::{implement, AsImpl, Ref, HRESULT};

const FRAME_RATE: u32 = 30;
const SIZES: [(u32, u32); 3] = [(1920, 1080), (1280, 720), (3840, 2160)];

struct State {
    state: MF_STREAM_STATE,
    queue: Option<IMFMediaEventQueue>,
    descriptor: Option<IMFStreamDescriptor>,
    source: Option<IMFMediaSource>,
    allocator: Option<IMFVideoSampleAllocatorEx>,
    media_type: Option<IMFMediaType>,
    size: (u32, u32),
    reader: Option<FrameReader>,
}

#[implement(IMFMediaStream2, IMFAttributes, IKsControl)]
pub struct Stream {
    attrs: IMFAttributes,
    inner: Mutex<State>,
}

forward_attributes!(Stream_Impl);

fn nv12_type(w: u32, h: u32) -> windows_core::Result<IMFMediaType> {
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        t.SetUINT64(&MF_MT_FRAME_SIZE, ((w as u64) << 32) | h as u64)?;
        t.SetUINT64(&MF_MT_FRAME_RATE, ((FRAME_RATE as u64) << 32) | 1)?;
        t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1 << 32) | 1)?;
        t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        t.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
        t.SetUINT32(&MF_MT_DEFAULT_STRIDE, w)?;
        t.SetUINT32(&MF_MT_AVG_BITRATE, w * h * 12 * FRAME_RATE)?;
        Ok(t)
    }
}

pub fn create(source: IMFMediaSource) -> windows_core::Result<IMFMediaStream2> {
    unsafe {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 4)?;
        let attrs = attrs.unwrap();
        attrs.SetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY, &PINNAME_VIDEO_CAPTURE)?;
        attrs.SetUINT32(&MF_DEVICESTREAM_STREAM_ID, 0)?;
        attrs.SetUINT32(&MF_DEVICESTREAM_FRAMESERVER_SHARED, 1)?;
        attrs.SetUINT32(&MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES, MFFrameSourceTypes_Color.0 as u32)?;

        let types: Vec<Option<IMFMediaType>> = SIZES.iter().map(|&(w, h)| nv12_type(w, h).ok()).collect();
        let descriptor = MFCreateStreamDescriptor(0, &types)?;
        descriptor.GetMediaTypeHandler()?.SetCurrentMediaType(types[0].as_ref().unwrap())?;

        Ok(Stream {
            attrs,
            inner: Mutex::new(State {
                state: MF_STREAM_STATE_STOPPED,
                queue: Some(MFCreateEventQueue()?),
                descriptor: Some(descriptor),
                source: Some(source),
                allocator: None,
                media_type: None,
                size: SIZES[0],
                reader: None,
            }),
        }
        .into())
    }
}

impl State {
    fn queue(&self) -> windows_core::Result<&IMFMediaEventQueue> {
        self.queue.as_ref().ok_or_else(shutdown_err)
    }

    fn queue_event(&self, event: MF_EVENT_TYPE) -> windows_core::Result<()> {
        unsafe { self.queue()?.QueueEventParamVar(event.0 as u32, &GUID::zeroed(), S_OK, std::ptr::null()) }
    }

    fn start(&mut self, media_type: Option<IMFMediaType>) -> windows_core::Result<()> {
        if let Some(t) = media_type {
            self.media_type = Some(t);
        }
        let t = self.media_type.clone().ok_or_else(|| windows_core::Error::from(MF_E_INVALIDMEDIATYPE))?;
        let packed = unsafe { t.GetUINT64(&MF_MT_FRAME_SIZE)? };
        self.size = ((packed >> 32) as u32, packed as u32);
        let allocator = self.allocator.as_ref().ok_or_else(|| windows_core::Error::from(MF_E_NOT_INITIALIZED))?;
        unsafe { allocator.InitializeSampleAllocator(10, &t)? };
        self.reader.get_or_insert_with(|| FrameReader::new(opencamera_vcam::shm::GLOBAL_NAME));
        self.queue_event(MEStreamStarted)?;
        self.state = MF_STREAM_STATE_RUNNING;
        Ok(())
    }

    fn stop(&mut self) -> windows_core::Result<()> {
        if let Some(a) = &self.allocator {
            unsafe { a.UninitializeSampleAllocator()? };
        }
        self.queue_event(MEStreamStopped)?;
        self.state = MF_STREAM_STATE_STOPPED;
        Ok(())
    }
}

/// Lets the camera source start, stop and shut down its video stream.
pub trait StreamControl {
    fn start(&self, media_type: IMFMediaType) -> windows_core::Result<()>;
    fn stop(&self) -> windows_core::Result<()>;
    fn set_allocator(&self, allocator: IMFVideoSampleAllocatorEx);
    fn shutdown(&self);
    fn is_running(&self) -> bool;
}

impl StreamControl for IMFMediaStream2 {
    fn start(&self, media_type: IMFMediaType) -> windows_core::Result<()> {
        inner(self).lock().unwrap().start(Some(media_type))
    }
    fn stop(&self) -> windows_core::Result<()> {
        inner(self).lock().unwrap().stop()
    }
    fn set_allocator(&self, allocator: IMFVideoSampleAllocatorEx) {
        inner(self).lock().unwrap().allocator = Some(allocator);
    }
    fn shutdown(&self) {
        let mut s = inner(self).lock().unwrap();
        if let Some(q) = s.queue.take() {
            let _ = unsafe { q.Shutdown() };
        }
        s.descriptor = None;
        s.source = None;
        s.allocator = None;
        s.reader = None;
    }
    fn is_running(&self) -> bool {
        inner(self).lock().unwrap().state != MF_STREAM_STATE_STOPPED
    }
}

fn inner(stream: &IMFMediaStream2) -> &Mutex<State> {
    // Safe because every stream our source handles was made by `create` above.
    let imp: &Stream = unsafe { stream.as_impl() };
    &imp.inner
}

impl IMFMediaEventGenerator_Impl for Stream_Impl {
    fn GetEvent(&self, flags: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS) -> windows_core::Result<IMFMediaEvent> {
        let q = self.inner.lock().unwrap().queue()?.clone();
        unsafe { q.GetEvent(flags.0) }
    }
    fn BeginGetEvent(&self, cb: Ref<IMFAsyncCallback>, state: Ref<windows_core::IUnknown>) -> windows_core::Result<()> {
        let q = self.inner.lock().unwrap().queue()?.clone();
        unsafe { q.BeginGetEvent(cb.as_ref(), state.as_ref()) }
    }
    fn EndGetEvent(&self, result: Ref<IMFAsyncResult>) -> windows_core::Result<IMFMediaEvent> {
        let q = self.inner.lock().unwrap().queue()?.clone();
        unsafe { q.EndGetEvent(result.as_ref()) }
    }
    fn QueueEvent(&self, met: u32, ext: *const GUID, hr: HRESULT, value: *const windows::Win32::System::Com::StructuredStorage::PROPVARIANT) -> windows_core::Result<()> {
        let q = self.inner.lock().unwrap().queue()?.clone();
        unsafe { q.QueueEventParamVar(met, ext, hr, value) }
    }
}

impl IMFMediaStream_Impl for Stream_Impl {
    fn GetMediaSource(&self) -> windows_core::Result<IMFMediaSource> {
        self.inner.lock().unwrap().source.clone().ok_or_else(shutdown_err)
    }

    fn GetStreamDescriptor(&self) -> windows_core::Result<IMFStreamDescriptor> {
        self.inner.lock().unwrap().descriptor.clone().ok_or_else(shutdown_err)
    }

    fn RequestSample(&self, token: Ref<windows_core::IUnknown>) -> windows_core::Result<()> {
        let mut s = self.inner.lock().unwrap();
        let allocator = s.allocator.clone().ok_or_else(shutdown_err)?;
        let queue = s.queue()?.clone();
        let (w, h) = (s.size.0 as usize, s.size.1 as usize);
        unsafe {
            let sample = allocator.AllocateSample()?;
            sample.SetSampleTime(MFGetSystemTime())?;
            sample.SetSampleDuration(10_000_000 / FRAME_RATE as i64)?;
            let buffer: IMF2DBuffer2 = sample.GetBufferByIndex(0)?.cast()?;
            let (mut scanline, mut pitch, mut start, mut len) = (std::ptr::null_mut(), 0i32, std::ptr::null_mut(), 0u32);
            buffer.Lock2DSize(MF2DBuffer_LockFlags_Write, &mut scanline, &mut pitch, &mut start, &mut len)?;
            if let Some(reader) = s.reader.as_mut() {
                reader.fill_nv12(scanline, pitch.unsigned_abs() as usize, w, h);
            }
            buffer.Unlock2D()?;
            if let Some(t) = token.as_ref() {
                sample.SetUnknown(&MFSampleExtension_Token, t)?;
            }
            queue.QueueEventParamUnk(MEMediaSample.0 as u32, &GUID::zeroed(), S_OK, &sample)
        }
    }
}

impl IMFMediaStream2_Impl for Stream_Impl {
    fn SetStreamState(&self, value: MF_STREAM_STATE) -> windows_core::Result<()> {
        let mut s = self.inner.lock().unwrap();
        if s.state == value {
            return Ok(());
        }
        match value {
            MF_STREAM_STATE_PAUSED if s.state == MF_STREAM_STATE_RUNNING => {
                s.state = value;
                Ok(())
            }
            MF_STREAM_STATE_RUNNING => s.start(None),
            MF_STREAM_STATE_STOPPED => s.stop(),
            _ => Err(MF_E_INVALID_STATE_TRANSITION.into()),
        }
    }

    fn GetStreamState(&self) -> windows_core::Result<MF_STREAM_STATE> {
        Ok(self.inner.lock().unwrap().state)
    }
}

impl IKsControl_Impl for Stream_Impl {
    fn KsProperty(&self, _: *const KSIDENTIFIER, _: u32, _: *mut core::ffi::c_void, _: u32, _: *mut u32) -> windows_core::Result<()> {
        Err(crate::ks_not_found())
    }
    fn KsMethod(&self, _: *const KSIDENTIFIER, _: u32, _: *mut core::ffi::c_void, _: u32, _: *mut u32) -> windows_core::Result<()> {
        Err(crate::ks_not_found())
    }
    fn KsEvent(&self, _: *const KSIDENTIFIER, _: u32, _: *mut core::ffi::c_void, _: u32, _: *mut u32) -> windows_core::Result<()> {
        Err(crate::ks_not_found())
    }
}
