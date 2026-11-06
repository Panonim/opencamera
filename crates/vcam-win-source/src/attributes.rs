//! Macro that passes every attribute call straight through to the object's own `attrs` store.

#[macro_export]
macro_rules! forward_attributes {
    ($impl:ty) => {
        impl windows::Win32::Media::MediaFoundation::IMFAttributes_Impl for $impl {
            fn GetItem(&self, k: *const GUID, v: *mut windows::Win32::System::Com::StructuredStorage::PROPVARIANT) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().GetItem)(self.attrs.as_raw(), k, v).ok() }
            }
            fn GetItemType(&self, k: *const GUID) -> windows_core::Result<MF_ATTRIBUTE_TYPE> {
                let mut out = Default::default();
                unsafe { (self.attrs.vtable().GetItemType)(self.attrs.as_raw(), k, &mut out).map(|| out) }
            }
            fn CompareItem(&self, k: *const GUID, v: *const windows::Win32::System::Com::StructuredStorage::PROPVARIANT) -> windows_core::Result<windows_core::BOOL> {
                let mut out = Default::default();
                unsafe { (self.attrs.vtable().CompareItem)(self.attrs.as_raw(), k, v, &mut out).map(|| out) }
            }
            fn Compare(&self, theirs: windows_core::Ref<IMFAttributes>, t: MF_ATTRIBUTES_MATCH_TYPE) -> windows_core::Result<windows_core::BOOL> {
                let mut out = Default::default();
                unsafe { (self.attrs.vtable().Compare)(self.attrs.as_raw(), std::mem::transmute_copy(&theirs), t, &mut out).map(|| out) }
            }
            fn GetUINT32(&self, k: *const GUID) -> windows_core::Result<u32> {
                let mut out = 0;
                unsafe { (self.attrs.vtable().GetUINT32)(self.attrs.as_raw(), k, &mut out).map(|| out) }
            }
            fn GetUINT64(&self, k: *const GUID) -> windows_core::Result<u64> {
                let mut out = 0;
                unsafe { (self.attrs.vtable().GetUINT64)(self.attrs.as_raw(), k, &mut out).map(|| out) }
            }
            fn GetDouble(&self, k: *const GUID) -> windows_core::Result<f64> {
                let mut out = 0.0;
                unsafe { (self.attrs.vtable().GetDouble)(self.attrs.as_raw(), k, &mut out).map(|| out) }
            }
            fn GetGUID(&self, k: *const GUID) -> windows_core::Result<GUID> {
                let mut out = GUID::zeroed();
                unsafe { (self.attrs.vtable().GetGUID)(self.attrs.as_raw(), k, &mut out).map(|| out) }
            }
            fn GetStringLength(&self, k: *const GUID) -> windows_core::Result<u32> {
                let mut out = 0;
                unsafe { (self.attrs.vtable().GetStringLength)(self.attrs.as_raw(), k, &mut out).map(|| out) }
            }
            fn GetString(&self, k: *const GUID, buf: windows_core::PWSTR, size: u32, len: *mut u32) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().GetString)(self.attrs.as_raw(), k, buf, size, len).ok() }
            }
            fn GetAllocatedString(&self, k: *const GUID, out: *mut windows_core::PWSTR, len: *mut u32) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().GetAllocatedString)(self.attrs.as_raw(), k, out, len).ok() }
            }
            fn GetBlobSize(&self, k: *const GUID) -> windows_core::Result<u32> {
                let mut out = 0;
                unsafe { (self.attrs.vtable().GetBlobSize)(self.attrs.as_raw(), k, &mut out).map(|| out) }
            }
            fn GetBlob(&self, k: *const GUID, buf: *mut u8, size: u32, len: *mut u32) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().GetBlob)(self.attrs.as_raw(), k, buf, size, len).ok() }
            }
            fn GetAllocatedBlob(&self, k: *const GUID, buf: *mut *mut u8, len: *mut u32) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().GetAllocatedBlob)(self.attrs.as_raw(), k, buf, len).ok() }
            }
            fn GetUnknown(&self, k: *const GUID, riid: *const GUID, ppv: *mut *mut core::ffi::c_void) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().GetUnknown)(self.attrs.as_raw(), k, riid, ppv).ok() }
            }
            fn SetItem(&self, k: *const GUID, v: *const windows::Win32::System::Com::StructuredStorage::PROPVARIANT) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().SetItem)(self.attrs.as_raw(), k, v).ok() }
            }
            fn DeleteItem(&self, k: *const GUID) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().DeleteItem)(self.attrs.as_raw(), k).ok() }
            }
            fn DeleteAllItems(&self) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().DeleteAllItems)(self.attrs.as_raw()).ok() }
            }
            fn SetUINT32(&self, k: *const GUID, v: u32) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().SetUINT32)(self.attrs.as_raw(), k, v).ok() }
            }
            fn SetUINT64(&self, k: *const GUID, v: u64) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().SetUINT64)(self.attrs.as_raw(), k, v).ok() }
            }
            fn SetDouble(&self, k: *const GUID, v: f64) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().SetDouble)(self.attrs.as_raw(), k, v).ok() }
            }
            fn SetGUID(&self, k: *const GUID, v: *const GUID) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().SetGUID)(self.attrs.as_raw(), k, v).ok() }
            }
            fn SetString(&self, k: *const GUID, v: &windows_core::PCWSTR) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().SetString)(self.attrs.as_raw(), k, *v).ok() }
            }
            fn SetBlob(&self, k: *const GUID, buf: *const u8, size: u32) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().SetBlob)(self.attrs.as_raw(), k, buf, size).ok() }
            }
            fn SetUnknown(&self, k: *const GUID, v: windows_core::Ref<windows_core::IUnknown>) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().SetUnknown)(self.attrs.as_raw(), k, std::mem::transmute_copy(&v)).ok() }
            }
            fn LockStore(&self) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().LockStore)(self.attrs.as_raw()).ok() }
            }
            fn UnlockStore(&self) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().UnlockStore)(self.attrs.as_raw()).ok() }
            }
            fn GetCount(&self) -> windows_core::Result<u32> {
                let mut out = 0;
                unsafe { (self.attrs.vtable().GetCount)(self.attrs.as_raw(), &mut out).map(|| out) }
            }
            fn GetItemByIndex(&self, i: u32, k: *mut GUID, v: *mut windows::Win32::System::Com::StructuredStorage::PROPVARIANT) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().GetItemByIndex)(self.attrs.as_raw(), i, k, v).ok() }
            }
            fn CopyAllItems(&self, dest: windows_core::Ref<IMFAttributes>) -> windows_core::Result<()> {
                unsafe { (self.attrs.vtable().CopyAllItems)(self.attrs.as_raw(), std::mem::transmute_copy(&dest)).ok() }
            }
        }
    };
}
