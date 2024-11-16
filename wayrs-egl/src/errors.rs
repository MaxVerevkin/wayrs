use std::{fmt, io};

use crate::egl_ffi;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
pub enum Error {
    OldEgl(u32, u32),
    Egl(EglError),
    ExtensionUnsupported(&'static str),
    BadGbmAlloc,
    NotCurrentContext,
    Io(io::Error),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OldEgl(maj, min) => {
                write!(f, "EGL 1.4 is required, but {maj}.{min} is available")
            }
            Self::Egl(egl_error) => egl_error.fmt(f),
            Self::ExtensionUnsupported(ext) => write!(f, "extension {ext} is not supported"),
            Self::BadGbmAlloc => f.write_str("could not allocate GBM buffer"),
            Self::NotCurrentContext => {
                f.write_str("EglContext::release called for not current context")
            }
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<EglError> for Error {
    fn from(value: EglError) -> Self {
        Self::Egl(value)
    }
}

#[derive(Debug)]
pub enum EglError {
    /// The last function succeeded without error.
    Success,
    /// EGL is not initialized, or could not be initialized, for the specified EGL display connection.
    NotInitialized,
    /// EGL cannot access a requested resource (for example a context is bound in another thread).
    BadAccess,
    /// EGL failed to allocate resources for the requested operation.
    BadAlloc,
    /// An unrecognized attribute or attribute value was passed in the attribute list.
    BadAttribute,
    /// An EGLContext argument does not name a valid EGL rendering context.
    BadContext,
    /// An EGLConfig argument does not name a valid EGL frame buffer configuration.
    BadConfig,
    /// The current surface of the calling thread is a window, pixel buffer or pixmap that is no longer valid.
    BadCurrentSurface,
    /// An EGLDisplay argument does not name a valid EGL display connection.
    BadDisplay,
    /// An EGLSurface argument does not name a valid surface (window, pixel buffer or pixmap) configured for GL rendering.
    BadSurface,
    /// Arguments are inconsistent (for example, a valid context requires buffers not supplied by a valid surface).
    BadMatch,
    /// One or more argument values are invalid.
    BadParameter,
    /// A NativePixmapType argument does not refer to a valid native pixmap.
    BadNativePixmap,
    /// A NativeWindowType argument does not refer to a valid native window.
    BadNativeWindow,
    /// A power management event has occurred. The application must destroy all contexts and reinitialise OpenGL ES state and objects to continue rendering.
    ContextLost,
    /// Unknown EGL error.
    Unknown,
}

impl std::error::Error for EglError {}

impl fmt::Display for EglError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Success => "The last function succeeded without error.",
            Self::NotInitialized => "EGL is not initialized, or could not be initialized, for the specified EGL display connection.",
            Self::BadAccess => "EGL cannot access a requested resource (for example a context is bound in another thread).",
            Self::BadAlloc => "EGL failed to allocate resources for the requested operation.",
            Self::BadAttribute => "An unrecognized attribute or attribute value was passed in the attribute list.",
            Self::BadContext => "An EGLContext argument does not name a valid EGL rendering context.",
            Self::BadConfig => "An EGLConfig argument does not name a valid EGL frame buffer configuration.",
            Self::BadCurrentSurface => "The current surface of the calling thread is a window, pixel buffer or pixmap that is no longer valid.",
            Self::BadDisplay => "An EGLDisplay argument does not name a valid EGL display connection.",
            Self::BadSurface => "An EGLSurface argument does not name a valid surface (window, pixel buffer or pixmap) configured for GL rendering.",
            Self::BadMatch => "Arguments are inconsistent (for example, a valid context requires buffers not supplied by a valid surface).",
            Self::BadParameter => "One or more argument values are invalid.",
            Self::BadNativePixmap => "A NativePixmapType argument does not refer to a valid native pixmap.",
            Self::BadNativeWindow => "A NativeWindowType argument does not refer to a valid native window.",
            Self::ContextLost => "A power management event has occurred. The application must destroy all contexts and reinitialise OpenGL ES state and objects to continue rendering.",
            Self::Unknown => "Unknown EGL error.",
        })
    }
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
