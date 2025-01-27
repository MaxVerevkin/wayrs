use libc::dev_t;
use std::ffi::{c_int, CStr};
use std::io;

use crate::xf86drm_ffi;

/// A DRM device
///
/// # Intended usage
///
/// `zwp_linux_dmabuf_feedback_v1` advertises devices using `dev_t` integers. Use this struct to
/// compare devices (do not compare raw `dev_t` values) and get node paths, which should be passed
/// to [`EglDisplay::new`](crate::EglDisplay::new).
pub struct DrmDevice(xf86drm_ffi::drmDevicePtr);

impl DrmDevice {
    /// Try to create DRM device from its `dev_t`
    pub fn new_from_id(id: dev_t) -> io::Result<Self> {
        let mut dev_ptr = std::ptr::null_mut();
        let result = unsafe { xf86drm_ffi::drmGetDeviceFromDevId(id, 0, &mut dev_ptr) };
        if result < 0 {
            Err(io::Error::from_raw_os_error(-result as _))
        } else {
            assert!(!dev_ptr.is_null());
            Ok(Self(dev_ptr))
        }
    }

    /// Get a render node path, if supported.
    #[must_use]
    pub fn render_node(&self) -> Option<&CStr> {
        self.get_node(xf86drm_ffi::DRM_NODE_RENDER)
    }

    fn get_node(&self, node: c_int) -> Option<&CStr> {
        if self.as_ref().available_nodes & (1 << node) == 0 {
            None
        } else {
            Some(unsafe { CStr::from_ptr(*self.as_ref().nodes.offset(node as isize)) })
        }
    }

    fn as_ref(&self) -> &xf86drm_ffi::drmDevice {
        unsafe { &*self.0 }
    }
}

impl Drop for DrmDevice {
    fn drop(&mut self) {
        unsafe {
            xf86drm_ffi::drmFreeDevice(&mut self.0);
        }
    }
}

impl PartialEq for DrmDevice {
    fn eq(&self, other: &Self) -> bool {
        unsafe { xf86drm_ffi::drmDevicesEqual(self.0, other.0) != 0 }
    }
}

impl Eq for DrmDevice {}
