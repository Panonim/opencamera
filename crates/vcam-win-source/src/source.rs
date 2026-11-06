use crate::stream::{self, StreamControl};
use crate::{forward_attributes, shutdown_err};
use std::sync::Mutex;
use windows::core::{s, Interface, GUID, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Media::KernelStreaming::{IKsControl, IKsControl_Impl, KSCAMERAPROFILE_HighFrameRate, KSCAMERAPROFILE_Legacy, KSIDENTIFIER};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT as RAW_PROPVARIANT;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_core::{implement, AsImpl, Ref, BOOL, HRESULT};

#[implement(IMFActivate)]
struct Activator {
    attrs: IMFAttributes,
    source: Mutex<Option<IMFMediaSourceEx>>,
}

forward_attributes!(Activator_Impl);

pub fn create_activator(clsid: GUID) -> windows_core::Result<IMFActivate> {
    unsafe {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 2)?;
        let attrs = attrs.unwrap();
        attrs.SetUINT32(&MF_VIRTUALCAMERA_PROVIDE_ASSOCIATED_CAMERA_SOURCES, 1)?;
        attrs.SetGUID(&MFT_TRANSFORM_CLSID_Attribute, &clsid)?;
        let source = create_source(&attrs)?;
        Ok(Activator { attrs, source: Mutex::new(Some(source)) }.into())
    }
}

impl IMFActivate_Impl for Activator_Impl {
    fn ActivateObject(&self, riid: *const GUID, ppv: *mut *mut core::ffi::c_void) -> windows_core::Result<()> {
        let mut guard = self.source.lock().unwrap();
        if guard.is_none() {
            *guard = Some(create_source(&self.attrs)?);
        }
        unsafe { guard.as_ref().unwrap().query(riid, ppv).ok() }
    }
    fn ShutdownObject(&self) -> windows_core::Result<()> {
        Ok(())
    }
    fn DetachObject(&self) -> windows_core::Result<()> {
        *self.source.lock().unwrap() = None;
        Ok(())
    }
}

struct State {
    queue: Option<IMFMediaEventQueue>,
    descriptor: Option<IMFPresentationDescriptor>,
    streams: Vec<IMFMediaStream2>,
}

impl State {
    fn queue(&self) -> windows_core::Result<IMFMediaEventQueue> {
        self.queue.clone().ok_or_else(shutdown_err)
    }

    fn descriptor(&self) -> windows_core::Result<IMFPresentationDescriptor> {
        self.descriptor.clone().ok_or_else(shutdown_err)
    }
}

#[implement(IMFMediaSourceEx, IMFAttributes, IMFGetService, IKsControl, IMFSampleAllocatorControl)]
struct Source {
    attrs: IMFAttributes,
    inner: Mutex<State>,
}

forward_attributes!(Source_Impl);

fn create_source(activator_attrs: &IMFAttributes) -> windows_core::Result<IMFMediaSourceEx> {
    unsafe {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 4)?;
        let attrs = attrs.unwrap();
        activator_attrs.CopyAllItems(&attrs)?;

        if let Some(profiles) = sensor_profiles() {
            attrs.SetUnknown(&MF_DEVICEMFT_SENSORPROFILE_COLLECTION, &profiles)?;
        }

        let source: IMFMediaSourceEx = Source {
            attrs,
            inner: Mutex::new(State { queue: Some(MFCreateEventQueue()?), descriptor: None, streams: Vec::new() }),
        }
        .into();
        let stream = stream::create(source.cast()?)?;
        let descriptor = MFCreatePresentationDescriptor(Some(&[Some(stream.GetStreamDescriptor()?)]))?;
        {
            let imp: &Source = source.as_impl();
            let mut s = imp.inner.lock().unwrap();
            s.descriptor = Some(descriptor);
            s.streams.push(stream);
        }
        Ok(source)
    }
}

/// Builds the normal and high frame rate camera profiles, looked up at runtime so the DLL still loads on Windows 10.
fn sensor_profiles() -> Option<IMFSensorProfileCollection> {
    type CreateCollection = unsafe extern "system" fn(*mut *mut core::ffi::c_void) -> HRESULT;
    type CreateProfile = unsafe extern "system" fn(*const GUID, u32, PCWSTR, *mut *mut core::ffi::c_void) -> HRESULT;
    unsafe {
        let lib = LoadLibraryW(windows::core::w!("mfsensorgroup.dll")).ok()?;
        let create_collection: CreateCollection = std::mem::transmute(GetProcAddress(lib, s!("MFCreateSensorProfileCollection"))?);
        let create_profile: CreateProfile = std::mem::transmute(GetProcAddress(lib, s!("MFCreateSensorProfile"))?);
        let mut raw = std::ptr::null_mut();
        create_collection(&mut raw).ok().ok()?;
        let profiles = IMFSensorProfileCollection::from_raw(raw);
        for (kind, filter) in [
            (KSCAMERAPROFILE_Legacy, windows::core::w!("((RES==;FRT<=30,1;SUT==))")),
            (KSCAMERAPROFILE_HighFrameRate, windows::core::w!("((RES==;FRT>=60,1;SUT==))")),
        ] {
            let mut raw = std::ptr::null_mut();
            create_profile(&kind, 0, PCWSTR::null(), &mut raw).ok().ok()?;
            let profile = IMFSensorProfile::from_raw(raw);
            profile.AddProfileFilter(0, filter).ok()?;
            profiles.AddProfile(&profile).ok()?;
        }
        Some(profiles)
    }
}

fn queue_time_event(queue: &IMFMediaEventQueue, event: MF_EVENT_TYPE) -> windows_core::Result<()> {
    let time = RAW_PROPVARIANT::from(unsafe { MFGetSystemTime() });
    unsafe { queue.QueueEventParamVar(event.0 as u32, &GUID::zeroed(), S_OK, &time) }
}

impl IMFMediaEventGenerator_Impl for Source_Impl {
    fn GetEvent(&self, flags: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS) -> windows_core::Result<IMFMediaEvent> {
        let q = self.inner.lock().unwrap().queue()?;
        unsafe { q.GetEvent(flags.0) }
    }
    fn BeginGetEvent(&self, cb: Ref<IMFAsyncCallback>, state: Ref<windows_core::IUnknown>) -> windows_core::Result<()> {
        let q = self.inner.lock().unwrap().queue()?;
        unsafe { q.BeginGetEvent(cb.as_ref(), state.as_ref()) }
    }
    fn EndGetEvent(&self, result: Ref<IMFAsyncResult>) -> windows_core::Result<IMFMediaEvent> {
        let q = self.inner.lock().unwrap().queue()?;
        unsafe { q.EndGetEvent(result.as_ref()) }
    }
    fn QueueEvent(&self, met: u32, ext: *const GUID, hr: HRESULT, value: *const RAW_PROPVARIANT) -> windows_core::Result<()> {
        let q = self.inner.lock().unwrap().queue()?;
        unsafe { q.QueueEventParamVar(met, ext, hr, value) }
    }
}

impl IMFMediaSource_Impl for Source_Impl {
    fn GetCharacteristics(&self) -> windows_core::Result<u32> {
        Ok(MFMEDIASOURCE_IS_LIVE.0 as u32)
    }

    fn CreatePresentationDescriptor(&self) -> windows_core::Result<IMFPresentationDescriptor> {
        let descriptor = self.inner.lock().unwrap().descriptor()?;
        unsafe { descriptor.Clone() }
    }

    fn Start(&self, pd: Ref<IMFPresentationDescriptor>, time_format: *const GUID, _start: *const RAW_PROPVARIANT) -> windows_core::Result<()> {
        if !time_format.is_null() && unsafe { *time_format } != GUID::zeroed() {
            return Err(E_INVALIDARG.into());
        }
        let pd = pd.ok()?;
        let s = self.inner.lock().unwrap();
        let queue = s.queue()?;
        let ours = s.descriptor()?;
        unsafe {
            for i in 0..pd.GetStreamDescriptorCount()? {
                let (mut selected, mut desc) = (BOOL::default(), None);
                pd.GetStreamDescriptorByIndex(i, &mut selected, &mut desc)?;
                let desc = desc.ok_or_else(|| windows_core::Error::from(E_POINTER))?;
                let index = desc.GetStreamIdentifier()? as usize;
                let stream = s.streams.get(index).ok_or_else(|| windows_core::Error::from(E_INVALIDARG))?;
                if selected.as_bool() && !stream.is_running() {
                    ours.SelectStream(index as u32)?;
                    queue.QueueEventParamUnk(MENewStream.0 as u32, &GUID::zeroed(), S_OK, stream)?;
                    stream.start(desc.GetMediaTypeHandler()?.GetCurrentMediaType()?)?;
                } else if !selected.as_bool() && stream.is_running() {
                    ours.DeselectStream(index as u32)?;
                    stream.stop()?;
                }
            }
        }
        queue_time_event(&queue, MESourceStarted)
    }

    fn Stop(&self) -> windows_core::Result<()> {
        let s = self.inner.lock().unwrap();
        let queue = s.queue()?;
        let ours = s.descriptor()?;
        for (i, stream) in s.streams.iter().enumerate() {
            if stream.is_running() {
                stream.stop()?;
            }
            unsafe { ours.DeselectStream(i as u32)? };
        }
        queue_time_event(&queue, MESourceStopped)
    }

    fn Pause(&self) -> windows_core::Result<()> {
        Err(MF_E_INVALID_STATE_TRANSITION.into())
    }

    fn Shutdown(&self) -> windows_core::Result<()> {
        let mut s = self.inner.lock().unwrap();
        let queue = s.queue.take().ok_or_else(shutdown_err)?;
        let _ = unsafe { queue.Shutdown() };
        for stream in s.streams.drain(..) {
            stream.shutdown();
        }
        s.descriptor = None;
        Ok(())
    }
}

impl IMFMediaSourceEx_Impl for Source_Impl {
    fn GetSourceAttributes(&self) -> windows_core::Result<IMFAttributes> {
        Ok(self.attrs.clone())
    }

    fn GetStreamAttributes(&self, id: u32) -> windows_core::Result<IMFAttributes> {
        let s = self.inner.lock().unwrap();
        s.streams.get(id as usize).ok_or_else(|| windows_core::Error::from(E_INVALIDARG))?.cast()
    }

    fn SetD3DManager(&self, _: Ref<windows_core::IUnknown>) -> windows_core::Result<()> {
        // Ignores the graphics card helper because frames are made in regular memory.
        Ok(())
    }
}

impl IMFGetService_Impl for Source_Impl {
    fn GetService(&self, _: *const GUID, _: *const GUID, _: *mut *mut core::ffi::c_void) -> windows_core::Result<()> {
        Err(MF_E_UNSUPPORTED_SERVICE.into())
    }
}

impl IMFSampleAllocatorControl_Impl for Source_Impl {
    fn SetDefaultAllocator(&self, id: u32, allocator: Ref<windows_core::IUnknown>) -> windows_core::Result<()> {
        let s = self.inner.lock().unwrap();
        let stream = s.streams.get(id as usize).ok_or_else(|| windows_core::Error::from(E_INVALIDARG))?;
        stream.set_allocator(allocator.ok()?.cast()?);
        Ok(())
    }

    fn GetAllocatorUsage(&self, id: u32, input: *mut u32, usage: *mut MFSampleAllocatorUsage) -> windows_core::Result<()> {
        if input.is_null() || usage.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe {
            *input = id;
            *usage = MFSampleAllocatorUsage_UsesProvidedAllocator;
        }
        Ok(())
    }
}

impl IKsControl_Impl for Source_Impl {
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
