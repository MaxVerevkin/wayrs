use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::fmt;

use wayrs_client::Connection;
use wayrs_protocols::linux_dmabuf_unstable_v1::*;

use crate::{egl_ffi, gbm, Buffer, Error, Fourcc, GraphicsApi, Result, DRM_FORMAT_MOD_INVALID};

/// GBM-based EGL display
///
/// Dropping this struct terminates the EGL display.
// TODO: derive Debug when MSRV is >= 1.70
pub struct EglDisplay {
    raw: egl_ffi::EGLDisplay,
    gbm_device: gbm::Device,
    linux_dmabuf: ZwpLinuxDmabufV1,

    major_version: u32,
    minor_version: u32,

    extensions: EglExtensions,
    supported_formats: HashMap<Fourcc, Vec<u64>>,

    pub(crate) egl_image_target_renderbuffer_starage_oes:
        egl_ffi::EglImageTargetRenderbufferStorageOesProc,
}

impl EglDisplay {
    /// Create a new EGL display for a given DRM render node.
    pub fn new(linux_dmabuf: ZwpLinuxDmabufV1, drm_render_node: &CStr) -> Result<Self> {
        EglExtensions::query(egl_ffi::EGL_NO_DISPLAY)?.require("EGL_KHR_platform_gbm")?;

        let gbm_device = gbm::Device::open(drm_render_node)?;

        let raw = unsafe {
            egl_ffi::eglGetPlatformDisplay(
                egl_ffi::EGL_PLATFORM_GBM_KHR,
                gbm_device.as_raw() as *mut _,
                std::ptr::null(),
            )
        };

        if raw == egl_ffi::EGL_NO_DISPLAY {
            return Err(Error::last_egl());
        }

        let mut major_version = 0;
        let mut minor_version = 0;

        if unsafe { egl_ffi::eglInitialize(raw, &mut major_version, &mut minor_version) }
            != egl_ffi::EGL_TRUE
        {
            return Err(Error::last_egl());
        }

        if major_version <= 1 && minor_version < 5 {
            return Err(Error::OldEgl(major_version as u32, minor_version as u32));
        }

        let extensions = EglExtensions::query(raw)?;
        extensions.require("EGL_EXT_image_dma_buf_import_modifiers")?;
        extensions.require("EGL_KHR_no_config_context")?;
        extensions.require("EGL_KHR_surfaceless_context")?;

        let egl_query_dmabuf_formats_ext = unsafe {
            std::mem::transmute::<_, Option<egl_ffi::EglQueryDmabufFormatsExtProc>>(
                egl_ffi::eglGetProcAddress(b"eglQueryDmaBufFormatsEXT\0".as_ptr() as *const _),
            )
            .ok_or(Error::ExtensionUnsupported(
                "EGL_EXT_image_dma_buf_import_modifiers",
            ))?
        };

        let egl_query_dmabuf_modifiers_ext = unsafe {
            std::mem::transmute::<_, Option<egl_ffi::EglQueryDmabufModifiersExtProc>>(
                egl_ffi::eglGetProcAddress(b"eglQueryDmaBufModifiersEXT\0".as_ptr() as *const _),
            )
            .ok_or(Error::ExtensionUnsupported(
                "EGL_EXT_image_dma_buf_import_modifiers",
            ))?
        };

        // NOTE: eglGetProcAddress may return non-null pointer even if the extension is not supported.
        // Since this is a OpenGL/GLES extention, we cannot check it's presence now.
        let egl_image_target_renderbuffer_starage_oes = unsafe {
            std::mem::transmute::<_, Option<egl_ffi::EglImageTargetRenderbufferStorageOesProc>>(
                egl_ffi::eglGetProcAddress(
                    b"glEGLImageTargetRenderbufferStorageOES\0".as_ptr() as *const _
                ),
            )
            .ok_or(Error::ExtensionUnsupported("GL_OES_EGL_image"))?
        };

        let supported_formats = unsafe {
            get_supported_formats(
                raw,
                &gbm_device,
                egl_query_dmabuf_formats_ext,
                egl_query_dmabuf_modifiers_ext,
            )?
        };

        Ok(Self {
            raw,
            gbm_device,
            linux_dmabuf,

            major_version: major_version as u32,
            minor_version: minor_version as u32,

            extensions,
            supported_formats,

            egl_image_target_renderbuffer_starage_oes,
        })
    }

    pub(crate) fn as_raw(&self) -> egl_ffi::EGLDisplay {
        self.raw
    }

    pub(crate) fn gbm_device(&self) -> &gbm::Device {
        &self.gbm_device
    }

    pub(crate) fn linux_dmabuf(&self) -> ZwpLinuxDmabufV1 {
        self.linux_dmabuf
    }

    /// Major EGL version
    pub fn major_version(&self) -> u32 {
        self.major_version
    }

    /// Minor EGL version
    pub fn minor_version(&self) -> u32 {
        self.minor_version
    }

    /// The set of extensions this EGL display supports
    pub fn extensions(&self) -> &EglExtensions {
        &self.extensions
    }

    /// Get a set of supported buffer formats, in a form of fourcc -> modifiers mapping
    pub fn supported_formats(&self) -> &HashMap<Fourcc, Vec<u64>> {
        &self.supported_formats
    }

    /// Check whether a fourcc/modifier pair is supported
    pub fn is_format_supported(&self, fourcc: Fourcc, modifier: u64) -> bool {
        match self.supported_formats.get(&fourcc) {
            Some(mods) => {
                (mods.is_empty() && modifier == DRM_FORMAT_MOD_INVALID) || mods.contains(&modifier)
            }
            None => false,
        }
    }

    /// Allocate a new buffer
    pub fn alloc_buffer<D>(
        &self,
        conn: &mut Connection<D>,
        width: u32,
        height: u32,
        fourcc: Fourcc,
        modifiers: &[u64],
    ) -> Result<Buffer> {
        Buffer::alloc(self, conn, width, height, fourcc, modifiers)
    }
}

impl Drop for EglDisplay {
    fn drop(&mut self) {
        // SAFETY: terminating EGL display does not invalidate the display pointer, so objects
        // created from this display may outlive this struct and still reference this EGLDisplay.
        //
        // NOTE: `glutin` crate does not terminate EGL displays on drop because
        // eglGetPlatformDisplay returns the same pointer each time it is called with the same
        // arguments. This is a problem because two EglDisplay objects may be created referencing
        // the same EGLDisplay pointer, so dropping one display terminates another. However, this
        // is not a problem in our particular case because each time EglDisplay::new is called, a
        // new GBM device pointer is created. Even if two GMB devices represent the same resource,
        // the pointers are different, so eglGetPlatformDisplay must return a new EGLDisplay. GBM
        // device pointer probably may be reused after the device is freed, but this is again not
        // a problem because GBM device is kept alive for the lifetime of EglDisplay.
        unsafe { egl_ffi::eglTerminate(self.raw) };
    }
}

unsafe fn get_supported_formats(
    dpy: egl_ffi::EGLDisplay,
    gbm_device: &gbm::Device,
    qf: egl_ffi::EglQueryDmabufFormatsExtProc,
    qm: egl_ffi::EglQueryDmabufModifiersExtProc,
) -> Result<HashMap<Fourcc, Vec<u64>>> {
    let mut retval = HashMap::new();

    let mut formats_len = 0;
    if unsafe { qf(dpy, 0, std::ptr::null_mut(), &mut formats_len) } != egl_ffi::EGL_TRUE {
        return Err(Error::last_egl());
    }

    let mut formats_buf = Vec::with_capacity(formats_len as usize);
    if unsafe { qf(dpy, formats_len, formats_buf.as_mut_ptr(), &mut formats_len) }
        != egl_ffi::EGL_TRUE
    {
        return Err(Error::last_egl());
    }
    unsafe { formats_buf.set_len(formats_len as usize) };

    for &format in formats_buf
        .iter()
        .filter(|&&fmt| gbm_device.is_format_supported(Fourcc(fmt as u32)))
    {
        let mut mods_len = 0;
        if unsafe {
            qm(
                dpy,
                format,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut mods_len,
            )
        } != egl_ffi::EGL_TRUE
        {
            return Err(Error::last_egl());
        }

        let mut mods_buf = Vec::with_capacity(mods_len as usize);
        if unsafe {
            qm(
                dpy,
                format,
                mods_len,
                mods_buf.as_mut_ptr(),
                std::ptr::null_mut(),
                &mut mods_len,
            )
        } != egl_ffi::EGL_TRUE
        {
            return Err(Error::last_egl());
        }
        unsafe { mods_buf.set_len(mods_len as usize) };

        retval.insert(Fourcc(format as u32), mods_buf);
    }

    Ok(retval)
}

/// [`EglContext`] builder
pub struct EglContextBuilder {
    api: GraphicsApi,
    major_v: u32,
    minor_v: u32,
    debug: bool,
}

impl EglContextBuilder {
    /// Create a new [`EglContext`] builder
    pub fn new(api: GraphicsApi) -> Self {
        Self {
            api,
            major_v: 1,
            minor_v: 0,
            debug: false,
        }
    }

    /// Set the required API version. Default is `1.0`.
    pub fn version(mut self, major: u32, minor: u32) -> Self {
        self.major_v = major;
        self.minor_v = minor;
        self
    }

    /// Enable/disable debugging. Default is `false`.
    pub fn debug(mut self, enable: bool) -> Self {
        self.debug = enable;
        self
    }

    /// Create a new graphics API context
    ///
    /// Call [`EglContext::make_current`] to activate the context.
    pub fn build(self, display: &EglDisplay) -> Result<EglContext> {
        let api = match self.api {
            GraphicsApi::OpenGl => egl_ffi::EGL_OPENGL_API,
            GraphicsApi::OpenGlEs => egl_ffi::EGL_OPENGL_ES_API,
            GraphicsApi::OpenVg => egl_ffi::EGL_OPENVG_API,
        };

        if unsafe { egl_ffi::eglBindAPI(api) } != egl_ffi::EGL_TRUE {
            return Err(Error::last_egl());
        }

        let context_attrs = [
            egl_ffi::EGL_CONTEXT_MAJOR_VERSION,
            self.major_v as _,
            egl_ffi::EGL_CONTEXT_MINOR_VERSION,
            self.minor_v as _,
            egl_ffi::EGL_CONTEXT_OPENGL_DEBUG,
            self.debug as _,
            egl_ffi::EGL_NONE,
        ];

        let raw = unsafe {
            egl_ffi::eglCreateContext(
                display.raw,
                egl_ffi::EGL_NO_CONFIG,
                egl_ffi::EGL_NO_CONTEXT,
                context_attrs.as_ptr(),
            )
        };

        if raw == egl_ffi::EGL_NO_CONTEXT {
            return Err(Error::last_egl());
        }

        Ok(EglContext {
            raw,
            api,
            egl_display: display.raw,
        })
    }
}

/// EGL graphics API context
///
/// Call [`make_current`](Self::make_current) to activate the context. Dropping this struct will destroy the context if
/// it is not current on any thread. Otherwise it will be destroyed when it stops being current.
#[derive(Debug)]
pub struct EglContext {
    raw: egl_ffi::EGLContext,
    api: egl_ffi::EGLenum,
    egl_display: egl_ffi::EGLDisplay,
}

impl EglContext {
    /// Make this context current on the current therad.
    ///
    /// The context is [surfaceless][1], that is, it does not have a default framebuffer. You need
    /// to create GL framebuffer object (fbo), renderbuffer object (rbo) and allocate a number of
    /// [`Buffer`]s in order to render something.
    ///
    /// [1]: https://registry.khronos.org/EGL/extensions/KHR/EGL_KHR_surfaceless_context.txt
    pub fn make_current(&self) -> Result<()> {
        if unsafe {
            egl_ffi::eglMakeCurrent(
                self.egl_display,
                egl_ffi::EGL_NO_SURFACE,
                egl_ffi::EGL_NO_SURFACE,
                self.raw,
            )
        } != egl_ffi::EGL_TRUE
        {
            Err(Error::last_egl())
        } else {
            Ok(())
        }
    }

    /// Releases the current API context.
    ///
    /// If this context is not current on this thread, `Err(Error::NotCurrentContext)` is returned.
    pub fn release(&self) -> Result<()> {
        if unsafe { egl_ffi::eglGetCurrentContext() } != self.raw {
            return Err(Error::NotCurrentContext);
        }

        if unsafe { egl_ffi::eglBindAPI(self.api) } != egl_ffi::EGL_TRUE {
            return Err(Error::last_egl());
        }

        if unsafe {
            egl_ffi::eglMakeCurrent(
                self.egl_display,
                egl_ffi::EGL_NO_SURFACE,
                egl_ffi::EGL_NO_SURFACE,
                egl_ffi::EGL_NO_CONTEXT,
            )
        } != egl_ffi::EGL_TRUE
        {
            return Err(Error::last_egl());
        }

        Ok(())
    }
}

impl Drop for EglContext {
    fn drop(&mut self) {
        unsafe { egl_ffi::eglDestroyContext(self.egl_display, self.raw) };
    }
}

/// A set of EGL extensions
pub struct EglExtensions(HashSet<&'static [u8]>);

impl EglExtensions {
    pub(crate) fn query(display: egl_ffi::EGLDisplay) -> Result<Self> {
        let ptr = unsafe { egl_ffi::eglQueryString(display, egl_ffi::EGL_EXTENSIONS) };

        if ptr.is_null() {
            return Err(Error::last_egl());
        }

        let bytes = unsafe { CStr::from_ptr::<'static>(ptr) }.to_bytes();
        Ok(Self(bytes.split(|b| *b == b' ').collect()))
    }

    /// Check whether a given extension is supported
    pub fn contains(&self, ext: &str) -> bool {
        self.0.contains(ext.as_bytes())
    }

    /// Returns `Err(Error::ExtensionUnsupported(_))` if a given extension is not supported
    pub fn require(&self, ext: &'static str) -> Result<()> {
        if self.contains(ext) {
            Ok(())
        } else {
            Err(Error::ExtensionUnsupported(ext))
        }
    }
}

impl fmt::Debug for EglExtensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_set();
        for ext in &self.0 {
            let ext = String::from_utf8_lossy(ext);
            debug.entry(&ext.as_ref());
        }
        debug.finish()
    }
}
