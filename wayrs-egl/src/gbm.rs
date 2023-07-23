use std::ffi::CStr;
use std::io;
use std::os::unix::io::{FromRawFd, OwnedFd, RawFd};

use crate::{Error, Fourcc, Result};

#[derive(Debug)]
pub struct Device {
    raw: *mut gbm_sys::gbm_device,
    fd: RawFd,
}

impl Device {
    pub fn open(path: &CStr) -> io::Result<Self> {
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let raw = unsafe { gbm_sys::gbm_create_device(fd) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(Self { raw, fd })
    }

    pub fn as_raw(&self) -> *mut gbm_sys::gbm_device {
        self.raw
    }

    pub fn alloc_buffer(
        &self,
        width: u32,
        height: u32,
        fourcc: Fourcc,
        modifiers: &[u64],
    ) -> Result<Buffer> {
        let ptr = unsafe {
            gbm_sys::gbm_bo_create_with_modifiers2(
                self.raw,
                width,
                height,
                fourcc.0,
                modifiers.as_ptr(),
                modifiers.len() as u32,
                gbm_sys::gbm_bo_flags::GBM_BO_USE_RENDERING,
            )
        };
        if ptr.is_null() {
            Err(Error::BadGbmAlloc)
        } else {
            Ok(Buffer(ptr))
        }
    }

    pub fn is_format_supported(&self, fourcc: Fourcc) -> bool {
        unsafe {
            gbm_sys::gbm_device_is_format_supported(
                self.raw,
                fourcc.0,
                gbm_sys::gbm_bo_flags::GBM_BO_USE_RENDERING,
            ) != 0
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            gbm_sys::gbm_device_destroy(self.raw);
            libc::close(self.fd);
        }
    }
}

#[derive(Debug)]
pub struct Buffer(*mut gbm_sys::gbm_bo);

impl Buffer {
    pub fn export(&self) -> BufferExport {
        let num_planes = unsafe { gbm_sys::gbm_bo_get_plane_count(self.0) };
        let modifier = unsafe { gbm_sys::gbm_bo_get_modifier(self.0) };
        let mut planes = Vec::with_capacity(num_planes as usize);

        for i in 0..num_planes {
            let fd = unsafe { gbm_sys::gbm_bo_get_fd_for_plane(self.0, i) };
            let offset = unsafe { gbm_sys::gbm_bo_get_offset(self.0, i) };
            let stride = unsafe { gbm_sys::gbm_bo_get_stride_for_plane(self.0, i) };

            assert!(fd >= 0);

            planes.push(BufferPlane {
                dmabuf: unsafe { OwnedFd::from_raw_fd(fd) },
                offset,
                stride,
            });
        }

        BufferExport { modifier, planes }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { gbm_sys::gbm_bo_destroy(self.0) };
    }
}

#[derive(Debug)]
pub struct BufferExport {
    pub modifier: u64,
    pub planes: Vec<BufferPlane>,
}

#[derive(Debug)]
pub struct BufferPlane {
    pub dmabuf: OwnedFd,
    pub offset: u32,
    pub stride: u32,
}
