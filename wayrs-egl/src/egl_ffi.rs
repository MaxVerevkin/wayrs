use std::ffi::{c_char, c_uint, c_void};

pub type EGLBoolean = c_uint;
pub type EGLenum = c_uint;
pub type EGLint = i32;
pub type EGLAttrib = isize;

pub type EglQueryDmabufFormatsExtProc = unsafe extern "system" fn(
    dpy: EGLDisplay,
    max_formats: EGLint,
    formats: *mut EGLint,
    num_formats: *mut EGLint,
) -> EGLBoolean;

pub type EglQueryDmabufModifiersExtProc = unsafe extern "system" fn(
    dpy: EGLDisplay,
    format: EGLint,
    max_modifiers: EGLint,
    modifiers: *mut u64,
    external_only: *mut EGLBoolean,
    num_modifiers: *mut EGLint,
) -> EGLBoolean;

pub type EglImageTargetRenderbufferStorageOesProc =
    unsafe extern "system" fn(target: EGLenum, image: EGLImage);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct EGLDisplay(pub *mut c_void);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct EGLConfig(pub *mut c_void);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct EGLContext(pub *mut c_void);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct EGLSurface(pub *mut c_void);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct EGLClientBuffer(pub *mut c_void);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct EGLImage(pub *mut c_void);

pub const EGL_BAD_ACCESS: EGLint = 0x3002;
pub const EGL_BAD_ALLOC: EGLint = 0x3003;
pub const EGL_BAD_ATTRIBUTE: EGLint = 0x3004;
pub const EGL_BAD_CONFIG: EGLint = 0x3005;
pub const EGL_BAD_CONTEXT: EGLint = 0x3006;
pub const EGL_BAD_CURRENT_SURFACE: EGLint = 0x3007;
pub const EGL_BAD_DISPLAY: EGLint = 0x3008;
pub const EGL_BAD_MATCH: EGLint = 0x3009;
pub const EGL_BAD_NATIVE_PIXMAP: EGLint = 0x300A;
pub const EGL_BAD_NATIVE_WINDOW: EGLint = 0x300B;
pub const EGL_BAD_PARAMETER: EGLint = 0x300C;
pub const EGL_BAD_SURFACE: EGLint = 0x300D;
pub const EGL_CONTEXT_LOST: EGLint = 0x300E;
pub const EGL_CONTEXT_MAJOR_VERSION: EGLint = 0x3098;
pub const EGL_CONTEXT_MINOR_VERSION: EGLint = 0x30FB;
pub const EGL_CONTEXT_OPENGL_DEBUG: EGLint = 0x31B0;
pub const EGL_DEFAULT_DISPLAY: *mut c_void = std::ptr::null_mut();
pub const EGL_DMA_BUF_PLANE0_FD_EXT: EGLint = 0x3272;
pub const EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT: EGLint = 0x3444;
pub const EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT: EGLint = 0x3443;
pub const EGL_DMA_BUF_PLANE0_OFFSET_EXT: EGLint = 0x3273;
pub const EGL_DMA_BUF_PLANE0_PITCH_EXT: EGLint = 0x3274;
pub const EGL_DMA_BUF_PLANE1_FD_EXT: EGLint = 0x3275;
pub const EGL_DMA_BUF_PLANE1_MODIFIER_HI_EXT: EGLint = 0x3446;
pub const EGL_DMA_BUF_PLANE1_MODIFIER_LO_EXT: EGLint = 0x3445;
pub const EGL_DMA_BUF_PLANE1_OFFSET_EXT: EGLint = 0x3276;
pub const EGL_DMA_BUF_PLANE1_PITCH_EXT: EGLint = 0x3277;
pub const EGL_DMA_BUF_PLANE2_FD_EXT: EGLint = 0x3278;
pub const EGL_DMA_BUF_PLANE2_MODIFIER_HI_EXT: EGLint = 0x3448;
pub const EGL_DMA_BUF_PLANE2_MODIFIER_LO_EXT: EGLint = 0x3447;
pub const EGL_DMA_BUF_PLANE2_OFFSET_EXT: EGLint = 0x3279;
pub const EGL_DMA_BUF_PLANE2_PITCH_EXT: EGLint = 0x327A;
pub const EGL_DMA_BUF_PLANE3_FD_EXT: EGLint = 0x3440;
pub const EGL_DMA_BUF_PLANE3_MODIFIER_HI_EXT: EGLint = 0x344A;
pub const EGL_DMA_BUF_PLANE3_MODIFIER_LO_EXT: EGLint = 0x3449;
pub const EGL_DMA_BUF_PLANE3_OFFSET_EXT: EGLint = 0x3441;
pub const EGL_DMA_BUF_PLANE3_PITCH_EXT: EGLint = 0x3442;
pub const EGL_EXTENSIONS: EGLint = 0x3055;
pub const EGL_FALSE: EGLBoolean = 0;
pub const EGL_GL_RENDERBUFFER: EGLenum = 0x30B9;
pub const EGL_GL_TEXTURE_2D: EGLenum = 0x30B1;
pub const EGL_HEIGHT: EGLint = 0x3056;
pub const EGL_LINUX_DMA_BUF_EXT: EGLenum = 0x3270;
pub const EGL_LINUX_DRM_FOURCC_EXT: EGLint = 0x3271;
pub const EGL_NO_CONFIG: EGLConfig = EGLConfig(std::ptr::null_mut());
pub const EGL_NO_CONTEXT: EGLContext = EGLContext(std::ptr::null_mut());
pub const EGL_NO_DISPLAY: EGLDisplay = EGLDisplay(std::ptr::null_mut());
pub const EGL_NO_IMAGE: EGLImage = EGLImage(std::ptr::null_mut());
pub const EGL_NONE: EGLint = 0x3038;
pub const EGL_NO_SURFACE: EGLSurface = EGLSurface(std::ptr::null_mut());
pub const EGL_NOT_INITIALIZED: EGLint = 0x3001;
pub const EGL_OPENGL_API: EGLenum = 0x30A2;
pub const EGL_OPENGL_ES_API: EGLenum = 0x30A0;
pub const EGL_OPENVG_API: EGLenum = 0x30A1;
pub const EGL_PBUFFER_BIT: EGLint = 0x0001;
pub const EGL_PLATFORM_GBM_KHR: EGLenum = 0x31D7;
pub const EGL_SUCCESS: EGLint = 0x3000;
pub const EGL_SURFACE_TYPE: EGLint = 0x3033;
pub const EGL_TRUE: EGLBoolean = 1;
pub const EGL_WIDTH: EGLint = 0x3057;

pub const EGL_DMA_BUF_PLANE_FD_EXT: [EGLint; 4] = [
    EGL_DMA_BUF_PLANE0_FD_EXT,
    EGL_DMA_BUF_PLANE1_FD_EXT,
    EGL_DMA_BUF_PLANE2_FD_EXT,
    EGL_DMA_BUF_PLANE3_FD_EXT,
];
pub const EGL_DMA_BUF_PLANE_OFFSET_EXT: [EGLint; 4] = [
    EGL_DMA_BUF_PLANE0_OFFSET_EXT,
    EGL_DMA_BUF_PLANE1_OFFSET_EXT,
    EGL_DMA_BUF_PLANE2_OFFSET_EXT,
    EGL_DMA_BUF_PLANE3_OFFSET_EXT,
];
pub const EGL_DMA_BUF_PLANE_PITCH_EXT: [EGLint; 4] = [
    EGL_DMA_BUF_PLANE0_PITCH_EXT,
    EGL_DMA_BUF_PLANE1_PITCH_EXT,
    EGL_DMA_BUF_PLANE2_PITCH_EXT,
    EGL_DMA_BUF_PLANE3_PITCH_EXT,
];
pub const EGL_DMA_BUF_PLANE_MODIFIER_LO_EXT: [EGLint; 4] = [
    EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT,
    EGL_DMA_BUF_PLANE1_MODIFIER_LO_EXT,
    EGL_DMA_BUF_PLANE2_MODIFIER_LO_EXT,
    EGL_DMA_BUF_PLANE3_MODIFIER_LO_EXT,
];
pub const EGL_DMA_BUF_PLANE_MODIFIER_HI_EXT: [EGLint; 4] = [
    EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT,
    EGL_DMA_BUF_PLANE1_MODIFIER_HI_EXT,
    EGL_DMA_BUF_PLANE2_MODIFIER_HI_EXT,
    EGL_DMA_BUF_PLANE3_MODIFIER_HI_EXT,
];

#[link(name = "EGL")]
extern "C" {
    pub fn eglQueryString(dpy: EGLDisplay, name: EGLint) -> *const c_char;

    pub fn eglGetPlatformDisplay(
        platform: EGLenum,
        native_display: *mut c_void,
        attrib_list: *const EGLAttrib,
    ) -> EGLDisplay;

    pub fn eglInitialize(dpy: EGLDisplay, major: *mut EGLint, minor: *mut EGLint) -> EGLBoolean;

    pub fn eglTerminate(dpy: EGLDisplay) -> EGLBoolean;

    pub fn eglBindAPI(api: EGLenum) -> EGLBoolean;

    pub fn eglCreateContext(
        dpy: EGLDisplay,
        config: EGLConfig,
        share_context: EGLContext,
        attrib_list: *const EGLint,
    ) -> EGLContext;

    pub fn eglDestroyContext(dpy: EGLDisplay, context: EGLContext) -> EGLBoolean;

    pub fn eglMakeCurrent(
        dpy: EGLDisplay,
        draw: EGLSurface,
        read: EGLSurface,
        context: EGLContext,
    ) -> EGLBoolean;

    pub fn eglGetCurrentContext() -> EGLContext;

    pub fn eglCreateImage(
        dpy: EGLDisplay,
        context: EGLContext,
        target: EGLenum,
        buffer: EGLClientBuffer,
        attrib_list: *const EGLAttrib,
    ) -> EGLImage;

    pub fn eglDestroyImage(dpy: EGLDisplay, image: EGLImage) -> EGLBoolean;

    pub fn eglGetProcAddress(procname: *const c_char) -> *mut c_void;

    pub fn eglGetError() -> EGLint;
}
