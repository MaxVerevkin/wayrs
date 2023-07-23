#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int};

pub const DRM_NODE_RENDER: c_int = 2;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct drmDevice {
    pub nodes: *mut *mut c_char,
    pub available_nodes: c_int,
    // some fields omitted
}

pub type drmDevicePtr = *mut drmDevice;

extern "C" {
    pub fn drmGetDeviceFromDevId(
        dev_id: libc::dev_t,
        flags: u32,
        device: *mut drmDevicePtr,
    ) -> c_int;

    pub fn drmFreeDevice(device: *mut drmDevicePtr);

    pub fn drmDevicesEqual(a: drmDevicePtr, b: drmDevicePtr) -> c_int;
}
