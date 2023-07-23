//! Brings OpneGL-(ES) to `wayrs`.
//!
//! Requires EGL 1.5 with the following extensions:
//! - [`EGL_KHR_platform_gbm`][1]
//! - [`EGL_KHR_no_config_context`][2]
//! - [`EGL_KHR_surfaceless_context`][3]
//! - [`EGL_EXT_image_dma_buf_import_modifiers`][4]
//! - [`GL_OES_EGL_image`][5]
//!
//! [1]: https://registry.khronos.org/EGL/extensions/KHR/EGL_KHR_platform_gbm.txt
//! [2]: https://registry.khronos.org/EGL/extensions/KHR/EGL_KHR_no_config_context.txt
//! [3]: https://registry.khronos.org/EGL/extensions/KHR/EGL_KHR_surfaceless_context.txt
//! [4]: https://registry.khronos.org/EGL/extensions/EXT/EGL_EXT_image_dma_buf_import_modifiers.txt
//! [5]: https://registry.khronos.org/OpenGL/extensions/OES/OES_EGL_image.txt
//!
//! # Usage
//!
//! 1. Subscribe to `zwp_linux_dmabuf_feedback_v1` (for example, using `wayrs_utils::dmabuf_feedback::DmabufFeedback`).
//! 1. When feedback is received, get the render node path using [`DrmDevice`] and create [`EglDisplay`] for the given path.
//! 1. Select buffer formats that where advertised by dmabuf feedback and are supported by [`EglDisplay`]. From these formats choose the one you will use.
//! 1. Create [`EglContext`] using [`EglDisplay::create_context`] and make it current.
//! 1. Load graphics API functons using [`egl_ffi::eglGetProcAddress`].
//! 1. Assert that `GL_OES_EGL_image` is supported.
//! 1. Setup a framebuffer and a renderbuffer objects.
//!
//! Before rendering, allocate (or if you can reuse already allocated) [`Buffer`] and link it to your
//! renderbuffer object using [`Buffer::set_as_gl_renderbuffer_storage`]. After rendering, attach
//! and commit [`Buffer::wl_buffer`].
//!
//! See an example in [`examples/triangle.rs`](https://github.com/MaxVerevkin/wayrs/blob/main/wayrs-egl/examples/triangle.rs).

#![deny(unsafe_op_in_unsafe_fn)]

use std::fmt;

mod buffer;
mod drm;
mod egl;
mod errors;
mod gbm;
mod xf86drm_ffi;

pub mod egl_ffi;
pub use buffer::Buffer;
pub use drm::DrmDevice;
pub use egl::{EglContext, EglDisplay, EglExtensions};
pub use errors::*;

#[derive(Debug, Clone, Copy)]
pub enum GraphicsApi {
    OpenGl,
    OpenGlEs,
    OpenVg,
}

/// A DRM fourcc format wrapper with nice `Debug` formatting
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fourcc(pub u32);

impl fmt::Debug for Fourcc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.0.to_le_bytes();
        write!(
            f,
            "{}{}{}{}",
            a.escape_ascii(),
            b.escape_ascii(),
            c.escape_ascii(),
            d.escape_ascii()
        )
    }
}
