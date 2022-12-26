pub mod connection;
pub mod global;
pub mod interface;
pub mod object;
pub mod protocol;
pub mod proxy;
pub mod socket;
pub mod wire;

use std::ffi::CStr;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("both $XDG_RUNTIME_DIR and $WAYLAND_DISPLAY must by set")]
    NotEnoughEnvVars,
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[macro_export]
macro_rules! cstr {
    ($str:literal) => {{
        const X: &'static ::std::ffi::CStr = $crate::cstr(concat!($str, "\0"));
        X
    }};
}

// TODO: remove once `CStr::from_bytes_with_nul` becomes const-stable
pub const fn cstr(string: &str) -> &CStr {
    let bytes = string.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        assert!((byte != 0 && i + 1 != bytes.len()) || (byte == 0 && i + 1 == bytes.len()));
        i += 1;
    }

    // SAFETY: We've just checked that evey byte excepet the last one is not NULL.
    unsafe { CStr::from_bytes_with_nul_unchecked(bytes) }
}
