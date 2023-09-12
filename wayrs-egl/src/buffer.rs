use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex};

use wayrs_client::protocol::*;
use wayrs_client::Connection;
use wayrs_protocols::linux_dmabuf_unstable_v1::*;

use crate::{egl_ffi, EglDisplay, Error, Fourcc, Result};

/// A GBM-allocated buffer
///
/// A GBM-allocated buffer, which can be linked to GL renderbuffer objects and Wayland's
/// [`WlBuffer`]. To allocate a buffer, use [`EglDisplay::alloc_buffer`].
///
/// Buffers can and should be reused.
// TODO: derive Debug when MSRV is >= 1.70
pub struct Buffer {
    state: Arc<Mutex<BufferState>>,
    wl_buffer: WlBuffer,
    egl_display: egl_ffi::EGLDisplay,
    egl_image: egl_ffi::EGLImage,
    fourcc: Fourcc,
    modifier: u64,
    width: u32,
    height: u32,
    egl_image_target_renderbuffer_starage_oes: egl_ffi::EglImageTargetRenderbufferStorageOesProc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BufferState {
    /// The buffer is not in use
    Available,
    /// The buffer is used by the compositor
    InUse,
    /// The buffer is used by the compositor, but when it gets released, it should be destroyed
    PendingDestruction,
}

impl Buffer {
    pub(crate) fn alloc<D>(
        egl_display: &EglDisplay,
        conn: &mut Connection<D>,
        width: u32,
        height: u32,
        fourcc: Fourcc,
        modifiers: &[u64],
    ) -> Result<Self> {
        let raw_egl_display = egl_display.as_raw();

        let buf_parts = egl_display
            .gbm_device()
            .alloc_buffer(width, height, fourcc, modifiers)?
            .export();

        let mut egl_image_attrs = Vec::with_capacity(7 + 10 * buf_parts.planes.len());
        egl_image_attrs.push(egl_ffi::EGL_WIDTH as _);
        egl_image_attrs.push(width as _);
        egl_image_attrs.push(egl_ffi::EGL_HEIGHT as _);
        egl_image_attrs.push(height as _);
        egl_image_attrs.push(egl_ffi::EGL_LINUX_DRM_FOURCC_EXT as _);
        egl_image_attrs.push(fourcc.0 as _);
        for (i, plane) in buf_parts.planes.iter().enumerate() {
            egl_image_attrs.push(egl_ffi::EGL_DMA_BUF_PLANE_FD_EXT[i] as _);
            egl_image_attrs.push(plane.dmabuf.as_raw_fd() as _);
            egl_image_attrs.push(egl_ffi::EGL_DMA_BUF_PLANE_OFFSET_EXT[i] as _);
            egl_image_attrs.push(plane.offset as _);
            egl_image_attrs.push(egl_ffi::EGL_DMA_BUF_PLANE_PITCH_EXT[i] as _);
            egl_image_attrs.push(plane.stride as _);
            egl_image_attrs.push(egl_ffi::EGL_DMA_BUF_PLANE_MODIFIER_LO_EXT[i] as _);
            egl_image_attrs.push((buf_parts.modifier & 0xFFFF_FFFF) as _);
            egl_image_attrs.push(egl_ffi::EGL_DMA_BUF_PLANE_MODIFIER_HI_EXT[i] as _);
            egl_image_attrs.push((buf_parts.modifier >> 32) as _);
        }
        egl_image_attrs.push(egl_ffi::EGL_NONE as _);

        let egl_image = unsafe {
            egl_ffi::eglCreateImage(
                raw_egl_display,
                egl_ffi::EGL_NO_CONTEXT,
                egl_ffi::EGL_LINUX_DMA_BUF_EXT,
                egl_ffi::EGLClientBuffer(std::ptr::null_mut()),
                egl_image_attrs.as_ptr(),
            )
        };
        if egl_image == egl_ffi::EGL_NO_IMAGE {
            return Err(Error::last_egl());
        }

        let wl_buffer_params = egl_display.linux_dmabuf().create_params(conn);
        for (i, plane) in buf_parts.planes.into_iter().enumerate() {
            wl_buffer_params.add(
                conn,
                plane.dmabuf,
                i as u32,
                plane.offset,
                plane.stride,
                (buf_parts.modifier >> 32) as u32,
                (buf_parts.modifier & 0xFFFF_FFFF) as u32,
            );
        }
        let wl_buffer = wl_buffer_params.create_immed(
            conn,
            width as i32,
            height as i32,
            fourcc.0,
            zwp_linux_buffer_params_v1::Flags::empty(),
        );
        wl_buffer_params.destroy(conn);

        let state = Arc::new(Mutex::new(BufferState::Available));
        let state_copy = Arc::clone(&state);
        conn.set_callback_for(wl_buffer, move |ctx| {
            let mut state_guard = state_copy.lock().unwrap();
            match *state_guard {
                BufferState::Available => unreachable!(),
                BufferState::InUse => *state_guard = BufferState::Available,
                BufferState::PendingDestruction => ctx.proxy.destroy(ctx.conn),
            }
        });

        Ok(Buffer {
            state,
            wl_buffer,
            egl_display: raw_egl_display,
            egl_image,
            fourcc,
            modifier: buf_parts.modifier,
            width,
            height,
            egl_image_target_renderbuffer_starage_oes: egl_display
                .egl_image_target_renderbuffer_starage_oes,
        })
    }

    /// Get this buffer's fourcc format
    pub fn fourcc(&self) -> Fourcc {
        self.fourcc
    }

    /// Get this buffer format's modifier
    pub fn modifier(&self) -> u64 {
        self.modifier
    }

    /// Get this buffer's width
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get this buffer's height
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Check whether this buffer is currently in use by the compositor.
    pub fn is_available(&self) -> bool {
        *self.state.lock().unwrap() == BufferState::Available
    }

    /// Associate this buffer with a currently bound GL's renderbuffer object.
    ///
    /// This allows to render directly to this buffer.
    ///
    /// # Safety
    ///
    /// This function must be called from an OpenGL(-ES) context with support for [`GL_OES_EGL_image`][1]
    /// extension and a bound `GL_RENDERBUFFER`. Note that [`EglDisplay`](crate::EglDisplay) does not
    /// guarantee the presence of this extention.
    ///
    /// Rendering to a buffer that is currently in use by the compositor may cause visual glitches
    /// and may be considered UB.
    ///
    /// [1]: https://registry.khronos.org/OpenGL/extensions/OES/OES_EGL_image.txt
    pub unsafe fn set_as_gl_renderbuffer_storage(&self) {
        const GL_RENDERBUFFER: egl_ffi::EGLenum = 0x8D41;
        unsafe {
            (self.egl_image_target_renderbuffer_starage_oes)(GL_RENDERBUFFER, self.egl_image);
        }
    }

    /// Get a [`WlBuffer`] object which points to this buffer.
    ///
    /// This function marks the buffer as being in use, i.e. [`is_available`](Self::is_available)
    /// will return `false`.
    ///
    /// # Safety
    ///
    /// The returned [`WlBuffer`] object must be attached and commited to exactly one [`WlSurface`].
    ///
    /// # Panics
    ///
    /// This function will panic if this buffer is currently in use by the compositor.
    pub unsafe fn wl_buffer(&self) -> WlBuffer {
        let mut state_guard = self.state.lock().unwrap();
        assert_eq!(*state_guard, BufferState::Available, "buffer unavailable");
        *state_guard = BufferState::InUse;
        self.wl_buffer
    }

    /// Destroy this buffer.
    ///
    /// Not calling this function and just dropping the buffer will leak some resources.
    pub fn destroy<D>(self, conn: &mut Connection<D>) {
        let mut state_guard = self.state.lock().unwrap();
        match *state_guard {
            BufferState::Available => self.wl_buffer.destroy(conn),
            BufferState::InUse => *state_guard = BufferState::PendingDestruction,
            BufferState::PendingDestruction => unreachable!(),
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: EGLImage will not be used to create any new targets. Destroying an image does not
        // affect its "siblings", in our case the renderbuffer object. We ignore the result, since
        // there is not much we can do in case of an error.
        unsafe { egl_ffi::eglDestroyImage(self.egl_display, self.egl_image) };
    }
}

/// A pool of `N` buffers.
pub struct BufferPool<const N: usize> {
    buffers: [Option<Buffer>; N],
}

impl<const N: usize> Default for BufferPool<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> BufferPool<N> {
    /// Create a new buffer pool.
    pub fn new() -> Self {
        Self {
            buffers: std::array::from_fn(|_| None),
        }
    }

    /// Get a buffer, reusing free buffers if possible.
    ///
    /// Returns `Ok(None)` if all buffers are currently in use.
    pub fn get_buffer<D>(
        &mut self,
        egl_display: &EglDisplay,
        conn: &mut Connection<D>,
        width: u32,
        height: u32,
        fourcc: Fourcc,
        modifiers: &[u64],
    ) -> Result<Option<&Buffer>> {
        // Try to find a free, compatible buffer.
        for (i, buf) in self.buffers.iter().enumerate() {
            if let Some(buf) = buf {
                if buf.is_available()
                    && buf.width() == width
                    && buf.height() == height
                    && buf.fourcc() == fourcc
                    && modifiers.contains(&buf.modifier())
                {
                    return Ok(Some(self.buffers[i].as_ref().unwrap()));
                }
            }
        }

        // Try to find any free buffer.
        let buf_i = 'blk: {
            for (i, buf) in self.buffers.iter().enumerate() {
                if buf.as_ref().map_or(true, |b| b.is_available()) {
                    break 'blk i;
                }
            }
            return Ok(None);
        };

        if let Some(old_buf) = self.buffers[buf_i].take() {
            old_buf.destroy(conn);
        }

        Ok(Some(self.buffers[buf_i].insert(
            egl_display.alloc_buffer(conn, width, height, fourcc, modifiers)?,
        )))
    }

    /// Destroy all buffers in this pool.
    ///
    /// Not calling this function and just dropping the buffer pool will leak some resources.
    pub fn destroy<D>(self, conn: &mut Connection<D>) {
        for buf in self.buffers.into_iter().flatten() {
            buf.destroy(conn);
        }
    }
}
