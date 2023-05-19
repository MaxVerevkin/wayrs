//! A simple Rust implementation of Wayland client library

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod connection;
pub mod global;
pub mod interface;
pub mod object;
pub mod protocol;
pub mod proxy;
pub mod wire;

mod socket;

pub use wayrs_scanner as scanner;

use std::ffi::CStr;
use std::io;

/// An error that can occur while connecting to a Wayland socket.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// Either `$XDG_RUNTIME_DIR` or `$WAYLAND_DISPLAY` was not available.
    #[error("both $XDG_RUNTIME_DIR and $WAYLAND_DISPLAY must by set")]
    NotEnoughEnvVars,
    /// Some IO error.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The "mode" of an IO operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoMode {
    Blocking,
    NonBlocking,
}

/// Create a `&'static CStr` from a string literal. Panics at compile time if given string literal
/// contains null bytes.
#[macro_export]
macro_rules! cstr {
    ($str:literal) => {{
        const X: &'static ::std::ffi::CStr = $crate::cstr(concat!($str, "\0"));
        X
    }};
}

// TODO: remove once `CStr::from_bytes_with_nul` becomes const-stable
#[doc(hidden)]
pub const fn cstr(string: &str) -> &CStr {
    let bytes = string.as_bytes();
    assert!(!bytes.is_empty());

    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        assert!((byte != 0 && i + 1 != bytes.len()) || (byte == 0 && i + 1 == bytes.len()));
        i += 1;
    }

    // SAFETY: We've just checked that evey byte excepet the last one is not NULL.
    unsafe { CStr::from_bytes_with_nul_unchecked(bytes) }
}
