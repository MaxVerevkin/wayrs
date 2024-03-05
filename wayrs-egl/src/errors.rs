use std::io;

use crate::egl_ffi;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("EGL 1.4 is required, but {0}.{1} is available")]
    OldEgl(u32, u32),
    #[error(transparent)]
    Egl(#[from] EglError),
    #[error("extension {0} is not supported")]
    ExtensionUnsupported(&'static str),
    #[error("could not allocate GBM buffer")]
    BadGbmAlloc,
    #[error("EglContext::release called for not current context")]
    NotCurrentContext,
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum EglError {
    /// The last function succeeded without error.
    #[error("EGL_MESA_platform_surfaceless is not supported")]
    Success,
    /// EGL is not initialized, or could not be initialized, for the specified EGL display connection.
    #[error("EGL is not initialized, or could not be initialized, for the specified EGL display connection.")]
    NotInitialized,
    /// EGL cannot access a requested resource (for example a context is bound in another thread).
    #[error("EGL cannot access a requested resource (for example a context is bound in another thread).")]
    BadAccess,
    /// EGL failed to allocate resources for the requested operation.
    #[error("EGL failed to allocate resources for the requested operation.")]
    BadAlloc,
    /// An unrecognized attribute or attribute value was passed in the attribute list.
    #[error("An unrecognized attribute or attribute value was passed in the attribute list.")]
    BadAttribute,
    /// An EGLContext argument does not name a valid EGL rendering context.
    #[error("An EGLContext argument does not name a valid EGL rendering context.")]
    BadContext,
    /// An EGLConfig argument does not name a valid EGL frame buffer configuration.
    #[error("An EGLConfig argument does not name a valid EGL frame buffer configuration.")]
    BadConfig,
    /// The current surface of the calling thread is a window, pixel buffer or pixmap that is no longer valid.
    #[error("The current surface of the calling thread is a window, pixel buffer or pixmap that is no longer valid.")]
    BadCurrentSurface,
    /// An EGLDisplay argument does not name a valid EGL display connection.
    #[error("An EGLDisplay argument does not name a valid EGL display connection.")]
    BadDisplay,
    /// An EGLSurface argument does not name a valid surface (window, pixel buffer or pixmap) configured for GL rendering.
    #[error("An EGLSurface argument does not name a valid surface (window, pixel buffer or pixmap) configured for GL rendering.")]
    BadSurface,
    /// Arguments are inconsistent (for example, a valid context requires buffers not supplied by a valid surface).
    #[error("Arguments are inconsistent (for example, a valid context requires buffers not supplied by a valid surface).")]
    BadMatch,
    /// One or more argument values are invalid.
    #[error("One or more argument values are invalid.")]
    BadParameter,
    /// A NativePixmapType argument does not refer to a valid native pixmap.
    #[error("A NativePixmapType argument does not refer to a valid native pixmap.")]
    BadNativePixmap,
    /// A NativeWindowType argument does not refer to a valid native window.
    #[error("A NativeWindowType argument does not refer to a valid native window.")]
    BadNativeWindow,
    /// A power management event has occurred. The application must destroy all contexts and reinitialise OpenGL ES state and objects to continue rendering.
    #[error("A power management event has occurred. The application must destroy all contexts and reinitialise OpenGL ES state and objects to continue rendering.")]
    ContextLost,
    /// Unknown EGL error.
    #[error("Unknown EGL error.")]
    Unknown,
}

impl EglError {
    pub fn last() -> Self {
        match unsafe { egl_ffi::eglGetError() } {
            egl_ffi::EGL_SUCCESS => Self::Success,
            egl_ffi::EGL_NOT_INITIALIZED => Self::NotInitialized,
            egl_ffi::EGL_BAD_ACCESS => Self::BadAccess,
            egl_ffi::EGL_BAD_ALLOC => Self::BadAlloc,
            egl_ffi::EGL_BAD_ATTRIBUTE => Self::BadAttribute,
            egl_ffi::EGL_BAD_CONTEXT => Self::BadContext,
            egl_ffi::EGL_BAD_CONFIG => Self::BadConfig,
            egl_ffi::EGL_BAD_CURRENT_SURFACE => Self::BadCurrentSurface,
            egl_ffi::EGL_BAD_DISPLAY => Self::BadDisplay,
            egl_ffi::EGL_BAD_SURFACE => Self::BadSurface,
            egl_ffi::EGL_BAD_MATCH => Self::BadMatch,
            egl_ffi::EGL_BAD_PARAMETER => Self::BadParameter,
            egl_ffi::EGL_BAD_NATIVE_PIXMAP => Self::BadNativePixmap,
            egl_ffi::EGL_BAD_NATIVE_WINDOW => Self::BadNativeWindow,
            egl_ffi::EGL_CONTEXT_LOST => Self::ContextLost,
            _ => Self::Unknown,
        }
    }
}

impl Error {
    pub fn last_egl() -> Self {
        Self::Egl(EglError::last())
    }
}
