//! The Windows 10 virtual camera that camera apps load directly, fed with frames from shared memory.

use crate::frames::FrameReader;
use opencamera_vcam::{shm, CLSID_DSHOW};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::{Interface, GUID, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::BITMAPINFOHEADER;
use windows::Win32::Media::DirectShow::*;
use windows::Win32::Media::KernelStreaming::{IKsPropertySet, IKsPropertySet_Impl};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::Media::IReferenceClock;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, CoTaskMemFree, IPersist_Impl, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows_core::{implement, ComObjectInterface, Ref};

const PIN_NAME: &str = "Capture";
/// Time between frames at 30 frames per second, counted in 100-nanosecond steps.
const FRAME_INTERVAL: i64 = 333_333;
const AMPROPERTY_PIN_CATEGORY: u32 = 0;
const KSPROPERTY_SUPPORT_GET: u32 = 1;
const E_PROP_SET_UNSUPPORTED: HRESULT = HRESULT(0x8007_0492_u32 as i32);
const E_PROP_ID_UNSUPPORTED: HRESULT = HRESULT(0x8007_0490_u32 as i32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Subtype {
    Nv12,
    Yuy2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Format {
    subtype: Subtype,
    width: u32,
    height: u32,
}

const FORMATS: [Format; 6] = [
    Format { subtype: Subtype::Nv12, width: 1920, height: 1080 },
    Format { subtype: Subtype::Nv12, width: 1280, height: 720 },
    Format { subtype: Subtype::Nv12, width: 3840, height: 2160 },
    Format { subtype: Subtype::Yuy2, width: 1920, height: 1080 },
    Format { subtype: Subtype::Yuy2, width: 1280, height: 720 },
    Format { subtype: Subtype::Yuy2, width: 3840, height: 2160 },
];

impl Format {
    fn guid(&self) -> GUID {
        match self.subtype {
            Subtype::Nv12 => MEDIASUBTYPE_NV12,
            Subtype::Yuy2 => MEDIASUBTYPE_YUY2,
        }
    }

    fn bits(&self) -> u16 {
        match self.subtype {
            Subtype::Nv12 => 12,
            Subtype::Yuy2 => 16,
        }
    }

    fn image_size(&self) -> u32 {
        self.width * self.height * self.bits() as u32 / 8
    }

    fn bit_rate(&self) -> u32 {
        self.image_size() * 8 * 30
    }

    fn header(&self) -> VIDEOINFOHEADER {
        VIDEOINFOHEADER {
            dwBitRate: self.bit_rate(),
            AvgTimePerFrame: FRAME_INTERVAL,
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: self.width as i32,
                biHeight: self.height as i32,
                biPlanes: 1,
                biBitCount: self.bits(),
                biCompression: self.guid().data1,
                biSizeImage: self.image_size(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Describes this format for camera apps, which the caller must release with `free_media_type`.
    fn media_type(&self) -> AM_MEDIA_TYPE {
        let vih = unsafe { CoTaskMemAlloc(std::mem::size_of::<VIDEOINFOHEADER>()) as *mut VIDEOINFOHEADER };
        unsafe { vih.write(self.header()) };
        AM_MEDIA_TYPE {
            majortype: MEDIATYPE_Video,
            subtype: self.guid(),
            bFixedSizeSamples: true.into(),
            bTemporalCompression: false.into(),
            lSampleSize: self.image_size(),
            formattype: FORMAT_VideoInfo,
            pUnk: std::mem::ManuallyDrop::new(None),
            cbFormat: std::mem::size_of::<VIDEOINFOHEADER>() as u32,
            pbFormat: vih as *mut u8,
        }
    }

    /// Same as `media_type` but stored so the caller can take ownership of it.
    fn alloc_media_type(&self) -> *mut AM_MEDIA_TYPE {
        unsafe {
            let p = CoTaskMemAlloc(std::mem::size_of::<AM_MEDIA_TYPE>()) as *mut AM_MEDIA_TYPE;
            p.write(self.media_type());
            p
        }
    }

    /// Returns the supported format that exactly matches the given description, if any.
    unsafe fn from_media_type(mt: *const AM_MEDIA_TYPE) -> Option<Format> {
        let mt = mt.as_ref()?;
        if mt.majortype != MEDIATYPE_Video || mt.formattype != FORMAT_VideoInfo || mt.pbFormat.is_null() {
            return None;
        }
        if (mt.cbFormat as usize) < std::mem::size_of::<VIDEOINFOHEADER>() {
            return None;
        }
        let bmi = &(*(mt.pbFormat as *const VIDEOINFOHEADER)).bmiHeader;
        FORMATS
            .into_iter()
            .find(|f| f.guid() == mt.subtype && f.width as i32 == bmi.biWidth && f.height as i32 == bmi.biHeight.abs())
    }

    /// Checks whether a description, where blank fields mean "any", allows this format.
    unsafe fn allowed_by(&self, mt: *const AM_MEDIA_TYPE) -> bool {
        let Some(mt) = mt.as_ref() else { return true };
        (mt.majortype == GUID::zeroed() || mt.majortype == MEDIATYPE_Video)
            && (mt.subtype == GUID::zeroed() || mt.subtype == self.guid())
            && (mt.formattype == GUID::zeroed() || Format::from_media_type(mt) == Some(*self))
    }
}

unsafe fn free_media_type(mt: &mut AM_MEDIA_TYPE) {
    if !mt.pbFormat.is_null() {
        CoTaskMemFree(Some(mt.pbFormat as _));
        mt.pbFormat = std::ptr::null_mut();
    }
    std::mem::ManuallyDrop::drop(&mut mt.pUnk);
}

fn co_string(s: &str) -> PWSTR {
    let wide: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let p = CoTaskMemAlloc(wide.len() * 2) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        PWSTR(p)
    }
}

fn fixed_name(s: &[u16]) -> [u16; 128] {
    let mut out = [0u16; 128];
    let n = s.len().min(127);
    out[..n].copy_from_slice(&s[..n]);
    out
}

/// The background thread that sends frames, plus its stop signal.
struct Streaming {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

struct State {
    run_state: FILTER_STATE,
    clock: Option<IReferenceClock>,
    /// Borrowed link to the app's video pipeline, which owns this filter.
    graph: *mut core::ffi::c_void,
    name: Vec<u16>,
    start: i64,
    /// Borrowed link to the filter, cleared when the filter is destroyed.
    filter: *mut core::ffi::c_void,
    connected: Option<IPin>,
    input: Option<IMemInputPin>,
    allocator: Option<IMemAllocator>,
    format: Option<Format>,
    preferred: Option<Format>,
    streaming: Option<Streaming>,
}

unsafe impl Send for State {}

type Shared = Arc<Mutex<State>>;

struct SendPtr<T>(T);
unsafe impl<T> Send for SendPtr<T> {}

impl<T> SendPtr<T> {
    // Unwraps through a method so the thread takes the whole wrapper rather than its parts.
    fn into_inner(self) -> T {
        self.0
    }
}

#[implement(IBaseFilter, IAMFilterMiscFlags)]
struct Filter {
    shared: Shared,
    pin: IPin,
}

#[implement(IPin, IAMStreamConfig, IKsPropertySet)]
struct Pin {
    shared: Shared,
}

pub fn create_filter() -> windows_core::Result<IBaseFilter> {
    let shared: Shared = Arc::new(Mutex::new(State {
        run_state: State_Stopped,
        clock: None,
        graph: std::ptr::null_mut(),
        name: Vec::new(),
        start: 0,
        filter: std::ptr::null_mut(),
        connected: None,
        input: None,
        allocator: None,
        format: None,
        preferred: None,
        streaming: None,
    }));
    let pin: IPin = Pin { shared: shared.clone() }.into();
    let filter: IBaseFilter = Filter { shared: shared.clone(), pin }.into();
    shared.lock().unwrap().filter = filter.as_raw();
    Ok(filter)
}

/// Stops the frame thread, optionally releasing its buffers and waiting for it to end.
fn stop_streaming(shared: &Shared, decommit: bool, join: bool) {
    let (streaming, allocator) = {
        let mut s = shared.lock().unwrap();
        (s.streaming.take(), s.allocator.clone())
    };
    if let Some(st) = &streaming {
        st.stop.store(true, Ordering::SeqCst);
    }
    if decommit {
        if let Some(a) = allocator {
            // Releases the buffers so a thread waiting for one wakes up.
            let _ = unsafe { a.Decommit() };
        }
    }
    if let (true, Some(st)) = (join, streaming) {
        let _ = st.thread.join();
    }
}

fn start_streaming(shared: &Shared) {
    let mut s = shared.lock().unwrap();
    if s.streaming.is_some() {
        return;
    }
    let (Some(input), Some(allocator), Some(format)) = (s.input.clone(), s.allocator.clone(), s.format) else { return };
    let stop = Arc::new(AtomicBool::new(false));
    let ctx = SendPtr((input, allocator, s.clock.clone(), s.start, stop.clone()));
    let thread = std::thread::spawn(move || {
        let (input, allocator, clock, start, stop) = ctx.into_inner();
        push_loop(input, allocator, clock, start, format, stop);
    });
    s.streaming = Some(Streaming { stop, thread });
}

fn push_loop(input: IMemInputPin, allocator: IMemAllocator, clock: Option<IReferenceClock>, start: i64, format: Format, stop: Arc<AtomicBool>) {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let mut reader = FrameReader::new(shm::LOCAL_NAME);
    let (w, h) = (format.width as usize, format.height as usize);
    let interval = Duration::from_nanos(FRAME_INTERVAL as u64 * 100);
    let mut next = Instant::now();
    let mut frame: i64 = 0;
    while !stop.load(Ordering::SeqCst) {
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > interval * 4 {
            next = now;
        }
        next += interval;
        if stop.load(Ordering::SeqCst) {
            break;
        }
        unsafe {
            let mut sample = None;
            if allocator.GetBuffer(&mut sample, None, None, 0).is_err() {
                break;
            }
            let Some(sample) = sample else { break };
            let Ok(ptr) = sample.GetPointer() else { break };
            if (sample.GetSize() as u32) < format.image_size() {
                break;
            }
            match format.subtype {
                Subtype::Nv12 => reader.fill_nv12(ptr, w, w, h),
                Subtype::Yuy2 => reader.fill_yuy2(ptr, w * 2, w, h),
            }
            let _ = sample.SetActualDataLength(format.image_size() as i32);
            let clock_time = clock.as_ref().and_then(|c| c.GetTime().ok());
            let t0 = clock_time.map_or(frame * FRAME_INTERVAL, |t| (t - start).max(0));
            let t1 = t0 + FRAME_INTERVAL;
            let _ = sample.SetTime(Some(&t0), Some(&t1));
            let _ = sample.SetSyncPoint(true);
            let _ = sample.SetDiscontinuity(frame == 0);
            if input.Receive(&sample).is_err() {
                break;
            }
        }
        frame += 1;
    }
}

impl Drop for Filter {
    fn drop(&mut self) {
        stop_streaming(&self.shared, true, true);
        self.shared.lock().unwrap().filter = std::ptr::null_mut();
    }
}

impl IPersist_Impl for Filter_Impl {
    fn GetClassID(&self) -> windows_core::Result<GUID> {
        Ok(CLSID_DSHOW)
    }
}

impl IMediaFilter_Impl for Filter_Impl {
    fn Stop(&self) -> windows_core::Result<()> {
        stop_streaming(&self.shared, true, true);
        self.shared.lock().unwrap().run_state = State_Stopped;
        Ok(())
    }

    fn Pause(&self) -> windows_core::Result<()> {
        let (state, allocator) = {
            let s = self.shared.lock().unwrap();
            (s.run_state, s.allocator.clone())
        };
        if state == State_Stopped {
            if let Some(a) = allocator {
                unsafe { a.Commit()? };
            }
        } else if state == State_Running {
            // Stops without waiting because a paused app may be holding up the frame thread.
            stop_streaming(&self.shared, false, false);
        }
        self.shared.lock().unwrap().run_state = State_Paused;
        Ok(())
    }

    fn Run(&self, tstart: i64) -> windows_core::Result<()> {
        if self.shared.lock().unwrap().run_state == State_Stopped {
            self.Pause()?;
        }
        {
            let mut s = self.shared.lock().unwrap();
            s.start = tstart;
            s.run_state = State_Running;
        }
        start_streaming(&self.shared);
        Ok(())
    }

    fn GetState(&self, _timeout: u32) -> windows_core::Result<FILTER_STATE> {
        Ok(self.shared.lock().unwrap().run_state)
    }

    fn SetSyncSource(&self, clock: Ref<IReferenceClock>) -> windows_core::Result<()> {
        self.shared.lock().unwrap().clock = clock.cloned();
        Ok(())
    }

    fn GetSyncSource(&self) -> windows_core::Result<IReferenceClock> {
        self.shared.lock().unwrap().clock.clone().ok_or_else(|| VFW_E_NO_CLOCK.into())
    }
}

impl IBaseFilter_Impl for Filter_Impl {
    fn EnumPins(&self) -> windows_core::Result<IEnumPins> {
        Ok(PinEnum { pin: self.pin.clone(), pos: Mutex::new(0) }.into())
    }

    fn FindPin(&self, id: &PCWSTR) -> windows_core::Result<IPin> {
        if !id.is_null() && unsafe { id.to_string() }.is_ok_and(|s| s == PIN_NAME) {
            Ok(self.pin.clone())
        } else {
            Err(VFW_E_NOT_FOUND.into())
        }
    }

    fn QueryFilterInfo(&self, info: *mut FILTER_INFO) -> windows_core::Result<()> {
        let info = unsafe { info.as_mut() }.ok_or_else(|| windows_core::Error::from(E_POINTER))?;
        let s = self.shared.lock().unwrap();
        let graph = unsafe { IFilterGraph::from_raw_borrowed(&s.graph) }.cloned();
        unsafe { std::ptr::write(info, FILTER_INFO { achName: fixed_name(&s.name), pGraph: std::mem::ManuallyDrop::new(graph) }) };
        Ok(())
    }

    fn JoinFilterGraph(&self, graph: Ref<IFilterGraph>, name: &PCWSTR) -> windows_core::Result<()> {
        let mut s = self.shared.lock().unwrap();
        s.graph = graph.as_ref().map_or(std::ptr::null_mut(), |g| g.as_raw());
        s.name = if name.is_null() { Vec::new() } else { unsafe { name.as_wide() }.to_vec() };
        Ok(())
    }

    fn QueryVendorInfo(&self) -> windows_core::Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
}

impl IAMFilterMiscFlags_Impl for Filter_Impl {
    fn GetMiscFlags(&self) -> u32 {
        AM_FILTER_MISC_FLAGS_IS_SOURCE.0 as u32
    }
}

impl Pin_Impl {
    fn try_connect(&self, receiver: &IPin, format: Format) -> windows_core::Result<()> {
        let me: windows_core::InterfaceRef<IPin> = self.as_interface_ref();
        let mut mt = format.media_type();
        let result = unsafe { receiver.ReceiveConnection(&*me, &mt) };
        unsafe { free_media_type(&mut mt) };
        result?;
        match decide_allocator(receiver, format) {
            Ok((input, allocator)) => {
                let mut s = self.shared.lock().unwrap();
                s.connected = Some(receiver.clone());
                s.input = Some(input);
                s.allocator = Some(allocator);
                s.format = Some(format);
                Ok(())
            }
            Err(e) => {
                let _ = unsafe { receiver.Disconnect() };
                Err(e)
            }
        }
    }
}

/// Picks a buffer pool for frames, preferring the receiving app's own pool.
fn decide_allocator(receiver: &IPin, format: Format) -> windows_core::Result<(IMemInputPin, IMemAllocator)> {
    let input: IMemInputPin = receiver.cast()?;
    unsafe {
        let req = input.GetAllocatorRequirements().unwrap_or_default();
        let props = ALLOCATOR_PROPERTIES {
            cBuffers: req.cBuffers.max(3),
            cbBuffer: format.image_size() as i32,
            cbAlign: req.cbAlign.max(1),
            cbPrefix: req.cbPrefix,
        };
        let candidates = [input.GetAllocator().ok(), CoCreateInstance(&CLSID_MemoryAllocator, None, CLSCTX_INPROC_SERVER).ok()];
        for allocator in candidates.into_iter().flatten() {
            let fits = allocator.SetProperties(&props).is_ok_and(|actual| actual.cbBuffer >= props.cbBuffer);
            if fits && input.NotifyAllocator(&allocator, false).is_ok() {
                return Ok((input, allocator));
            }
        }
    }
    Err(VFW_E_NO_ALLOCATOR.into())
}

impl IPin_Impl for Pin_Impl {
    fn Connect(&self, receiver: Ref<IPin>, pmt: *const AM_MEDIA_TYPE) -> windows_core::Result<()> {
        let receiver = receiver.ok()?;
        let preferred = {
            let s = self.shared.lock().unwrap();
            if s.connected.is_some() {
                return Err(VFW_E_ALREADY_CONNECTED.into());
            }
            if s.run_state != State_Stopped {
                return Err(VFW_E_NOT_STOPPED.into());
            }
            s.preferred
        };
        let candidates = preferred.into_iter().chain(FORMATS.into_iter().filter(|f| Some(*f) != preferred));
        for format in candidates.filter(|f| unsafe { f.allowed_by(pmt) }) {
            if self.try_connect(receiver, format).is_ok() {
                return Ok(());
            }
        }
        Err(VFW_E_NO_ACCEPTABLE_TYPES.into())
    }

    fn ReceiveConnection(&self, _: Ref<IPin>, _: *const AM_MEDIA_TYPE) -> windows_core::Result<()> {
        Err(E_UNEXPECTED.into())
    }

    fn Disconnect(&self) -> windows_core::Result<()> {
        let mut s = self.shared.lock().unwrap();
        if s.run_state != State_Stopped {
            return Err(VFW_E_NOT_STOPPED.into());
        }
        if let Some(a) = s.allocator.take() {
            let _ = unsafe { a.Decommit() };
        }
        s.connected = None;
        s.input = None;
        s.format = None;
        Ok(())
    }

    fn ConnectedTo(&self) -> windows_core::Result<IPin> {
        self.shared.lock().unwrap().connected.clone().ok_or_else(|| VFW_E_NOT_CONNECTED.into())
    }

    fn ConnectionMediaType(&self, pmt: *mut AM_MEDIA_TYPE) -> windows_core::Result<()> {
        let pmt = unsafe { pmt.as_mut() }.ok_or_else(|| windows_core::Error::from(E_POINTER))?;
        match self.shared.lock().unwrap().format {
            Some(f) => {
                unsafe { std::ptr::write(pmt, f.media_type()) };
                Ok(())
            }
            None => {
                unsafe { std::ptr::write_bytes(pmt as *mut AM_MEDIA_TYPE, 0, 1) };
                Err(VFW_E_NOT_CONNECTED.into())
            }
        }
    }

    fn QueryPinInfo(&self, info: *mut PIN_INFO) -> windows_core::Result<()> {
        let info = unsafe { info.as_mut() }.ok_or_else(|| windows_core::Error::from(E_POINTER))?;
        let filter = {
            let s = self.shared.lock().unwrap();
            unsafe { IBaseFilter::from_raw_borrowed(&s.filter) }.cloned()
        };
        let name: Vec<u16> = PIN_NAME.encode_utf16().collect();
        unsafe {
            std::ptr::write(info, PIN_INFO { pFilter: std::mem::ManuallyDrop::new(filter), dir: PINDIR_OUTPUT, achName: fixed_name(&name) })
        };
        Ok(())
    }

    fn QueryDirection(&self) -> windows_core::Result<PIN_DIRECTION> {
        Ok(PINDIR_OUTPUT)
    }

    fn QueryId(&self) -> windows_core::Result<PWSTR> {
        Ok(co_string(PIN_NAME))
    }

    fn QueryAccept(&self, pmt: *const AM_MEDIA_TYPE) -> HRESULT {
        if unsafe { Format::from_media_type(pmt) }.is_some() {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn EnumMediaTypes(&self) -> windows_core::Result<IEnumMediaTypes> {
        // Offers only the chosen format once an app has picked one.
        let formats = match self.shared.lock().unwrap().preferred {
            Some(f) => vec![f],
            None => FORMATS.to_vec(),
        };
        Ok(MediaTypeEnum { formats, pos: Mutex::new(0) }.into())
    }

    fn QueryInternalConnections(&self, _: windows_core::OutRef<IPin>, _: *mut u32) -> windows_core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EndOfStream(&self) -> windows_core::Result<()> {
        Err(E_UNEXPECTED.into())
    }

    fn BeginFlush(&self) -> windows_core::Result<()> {
        Err(E_UNEXPECTED.into())
    }

    fn EndFlush(&self) -> windows_core::Result<()> {
        Err(E_UNEXPECTED.into())
    }

    fn NewSegment(&self, _: i64, _: i64, _: f64) -> windows_core::Result<()> {
        Ok(())
    }
}

impl IAMStreamConfig_Impl for Pin_Impl {
    fn SetFormat(&self, pmt: *const AM_MEDIA_TYPE) -> windows_core::Result<()> {
        if pmt.is_null() {
            self.shared.lock().unwrap().preferred = None;
            return Ok(());
        }
        let format = unsafe { Format::from_media_type(pmt) }.ok_or_else(|| windows_core::Error::from(VFW_E_INVALIDMEDIATYPE))?;
        let (reconnect, graph) = {
            let mut s = self.shared.lock().unwrap();
            s.preferred = Some(format);
            let reconnect = s.connected.is_some() && s.format != Some(format);
            if reconnect && s.run_state != State_Stopped {
                return Err(VFW_E_NOT_STOPPED.into());
            }
            (reconnect, unsafe { IFilterGraph::from_raw_borrowed(&s.graph) }.cloned())
        };
        if reconnect {
            let me: windows_core::InterfaceRef<IPin> = self.as_interface_ref();
            let graph = graph.ok_or_else(|| windows_core::Error::from(VFW_E_NOT_IN_GRAPH))?;
            unsafe { graph.Reconnect(&*me)? };
        }
        Ok(())
    }

    fn GetFormat(&self) -> windows_core::Result<*mut AM_MEDIA_TYPE> {
        let s = self.shared.lock().unwrap();
        Ok(s.format.or(s.preferred).unwrap_or(FORMATS[0]).alloc_media_type())
    }

    fn GetNumberOfCapabilities(&self, count: *mut i32, size: *mut i32) -> windows_core::Result<()> {
        if count.is_null() || size.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe {
            *count = FORMATS.len() as i32;
            *size = std::mem::size_of::<VIDEO_STREAM_CONFIG_CAPS>() as i32;
        }
        Ok(())
    }

    fn GetStreamCaps(&self, index: i32, ppmt: *mut *mut AM_MEDIA_TYPE, pscc: *mut u8) -> windows_core::Result<()> {
        let format = *FORMATS.get(index as usize).ok_or_else(|| windows_core::Error::from(E_INVALIDARG))?;
        if ppmt.is_null() || pscc.is_null() {
            return Err(E_POINTER.into());
        }
        let size = SIZE { cx: format.width as i32, cy: format.height as i32 };
        let bps = format.bit_rate().min(i32::MAX as u32) as i32;
        unsafe {
            *ppmt = format.alloc_media_type();
            std::ptr::write(
                pscc as *mut VIDEO_STREAM_CONFIG_CAPS,
                VIDEO_STREAM_CONFIG_CAPS {
                    guid: FORMAT_VideoInfo,
                    InputSize: size,
                    MinCroppingSize: size,
                    MaxCroppingSize: size,
                    CropGranularityX: 1,
                    CropGranularityY: 1,
                    CropAlignX: 1,
                    CropAlignY: 1,
                    MinOutputSize: size,
                    MaxOutputSize: size,
                    OutputGranularityX: 1,
                    OutputGranularityY: 1,
                    MinFrameInterval: FRAME_INTERVAL,
                    MaxFrameInterval: FRAME_INTERVAL,
                    MinBitsPerSecond: bps,
                    MaxBitsPerSecond: bps,
                    ..Default::default()
                },
            );
        }
        Ok(())
    }
}

/// Accepts only the property that asks which kind of output this pin is.
fn check_pin_category(set: *const GUID, id: u32) -> windows_core::Result<()> {
    if set.is_null() || unsafe { *set } != AMPROPSETID_Pin {
        return Err(E_PROP_SET_UNSUPPORTED.into());
    }
    if id != AMPROPERTY_PIN_CATEGORY {
        return Err(E_PROP_ID_UNSUPPORTED.into());
    }
    Ok(())
}

/// Tells apps this output is a capture pin, which is how they find the camera feed.
impl IKsPropertySet_Impl for Pin_Impl {
    fn Set(&self, _: *const GUID, _: u32, _: *const core::ffi::c_void, _: u32, _: *const core::ffi::c_void, _: u32) -> windows_core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn Get(&self, set: *const GUID, id: u32, _: *const core::ffi::c_void, _: u32, data: *mut core::ffi::c_void, cb: u32, returned: *mut u32) -> windows_core::Result<()> {
        check_pin_category(set, id)?;
        if !returned.is_null() {
            unsafe { *returned = std::mem::size_of::<GUID>() as u32 };
        }
        if data.is_null() {
            return Ok(());
        }
        if (cb as usize) < std::mem::size_of::<GUID>() {
            return Err(E_UNEXPECTED.into());
        }
        unsafe { *(data as *mut GUID) = PIN_CATEGORY_CAPTURE };
        Ok(())
    }

    fn QuerySupported(&self, set: *const GUID, id: u32) -> windows_core::Result<u32> {
        check_pin_category(set, id)?;
        Ok(KSPROPERTY_SUPPORT_GET)
    }
}

/// Reports how many items an enumerator returned and whether it returned all that were asked for.
fn finish_next(fetched: *mut u32, n: u32, count: u32) -> HRESULT {
    if !fetched.is_null() {
        unsafe { *fetched = n };
    }
    if n == count {
        S_OK
    } else {
        S_FALSE
    }
}

#[implement(IEnumPins)]
struct PinEnum {
    pin: IPin,
    pos: Mutex<u32>,
}

impl IEnumPins_Impl for PinEnum_Impl {
    fn Next(&self, count: u32, pins: *mut Option<IPin>, fetched: *mut u32) -> HRESULT {
        let mut pos = self.pos.lock().unwrap();
        let n = if count > 0 && *pos == 0 && !pins.is_null() {
            unsafe { pins.write(Some(self.pin.clone())) };
            *pos = 1;
            1
        } else {
            0
        };
        finish_next(fetched, n, count)
    }

    fn Skip(&self, count: u32) -> windows_core::Result<()> {
        let mut pos = self.pos.lock().unwrap();
        *pos = (*pos + count).min(1);
        Ok(())
    }

    fn Reset(&self) -> windows_core::Result<()> {
        *self.pos.lock().unwrap() = 0;
        Ok(())
    }

    fn Clone(&self) -> windows_core::Result<IEnumPins> {
        Ok(PinEnum { pin: self.pin.clone(), pos: Mutex::new(*self.pos.lock().unwrap()) }.into())
    }
}

#[implement(IEnumMediaTypes)]
struct MediaTypeEnum {
    formats: Vec<Format>,
    pos: Mutex<usize>,
}

impl IEnumMediaTypes_Impl for MediaTypeEnum_Impl {
    fn Next(&self, count: u32, types: *mut *mut AM_MEDIA_TYPE, fetched: *mut u32) -> HRESULT {
        let mut pos = self.pos.lock().unwrap();
        let mut n = 0u32;
        while n < count && *pos < self.formats.len() && !types.is_null() {
            unsafe { *types.add(n as usize) = self.formats[*pos].alloc_media_type() };
            *pos += 1;
            n += 1;
        }
        finish_next(fetched, n, count)
    }

    fn Skip(&self, count: u32) -> windows_core::Result<()> {
        let mut pos = self.pos.lock().unwrap();
        *pos = (*pos + count as usize).min(self.formats.len());
        Ok(())
    }

    fn Reset(&self) -> windows_core::Result<()> {
        *self.pos.lock().unwrap() = 0;
        Ok(())
    }

    fn Clone(&self) -> windows_core::Result<IEnumMediaTypes> {
        Ok(MediaTypeEnum { formats: self.formats.clone(), pos: Mutex::new(*self.pos.lock().unwrap()) }.into())
    }
}

/// Registers the filter as a camera device for all users, which needs admin rights.
pub unsafe fn register(name: &str) -> windows_core::Result<()> {
    let mapper: IFilterMapper2 = CoCreateInstance(&CLSID_FilterMapper2, None, CLSCTX_INPROC_SERVER)?;
    let types = [REGPINTYPES { clsMajorType: &MEDIATYPE_Video, clsMinorType: &MEDIASUBTYPE_NV12 }];
    let pins = [REGFILTERPINS {
        strName: PWSTR::null(),
        bRendered: false.into(),
        bOutput: true.into(),
        bZero: false.into(),
        bMany: false.into(),
        clsConnectsToFilter: &GUID::zeroed(),
        strConnectsToPin: PCWSTR::null(),
        nMediaTypes: types.len() as u32,
        lpMediaType: types.as_ptr(),
    }];
    let reg = REGFILTER2 {
        dwVersion: 1,
        dwMerit: MERIT_DO_NOT_USE.0 as u32,
        Anonymous: REGFILTER2_0 { Anonymous1: REGFILTER2_0_0 { cPins: pins.len() as u32, rgPins: pins.as_ptr() } },
    };
    let name = windows_core::HSTRING::from(name);
    let mut moniker = None;
    mapper.RegisterFilter(&CLSID_DSHOW, &name, Some(&mut moniker), &CLSID_VideoInputDeviceCategory, PCWSTR::null(), &reg)
}

pub unsafe fn unregister() {
    if let Ok(mapper) = CoCreateInstance::<_, IFilterMapper2>(&CLSID_FilterMapper2, None, CLSCTX_INPROC_SERVER) {
        let _ = mapper.UnregisterFilter(&CLSID_VideoInputDeviceCategory, PCWSTR::null(), &CLSID_DSHOW);
    }
}
