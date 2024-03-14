//! A simple Rust implementation of Wayland client library

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod global;
pub mod object;
pub mod protocol;

mod connection;
mod debug_message;

pub use connection::{ConnectError, Connection};

#[doc(hidden)]
pub use wayrs_scanner as _private_scanner;

pub use wayrs_core as core;
pub use wayrs_core::{Fixed, IoMode};

use std::ffi::CStr;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::{env, fmt};

pub use wayrs_core::transport::Transport;

pub trait ClientTransport: Transport {
    fn connect() -> Result<Self, ConnectError>
    where
        Self: Sized;

    fn fix_metadata(
        &mut self,
        plane_idx: usize,
        width: u32,
        height: u32,
        format: u32,
    ) -> Option<(u32, u32, u64)>;
}

impl ClientTransport for UnixStream {
    fn connect() -> Result<Self, ConnectError>
    where
        Self: Sized,
    {
        let runtime_dir = env::var_os("XDG_RUNTIME_DIR").ok_or(ConnectError::NotEnoughEnvVars)?;
        let wayland_disp = env::var_os("WAYLAND_DISPLAY").ok_or(ConnectError::NotEnoughEnvVars)?;

        let mut path = PathBuf::new();
        path.push(runtime_dir);
        path.push(wayland_disp);

        Ok(UnixStream::connect(path)?)
    }

    fn fix_metadata(
        &mut self,
        _plane_idx: usize,
        _width: u32,
        _height: u32,
        _format: u32,
    ) -> Option<(u32, u32, u64)> {
        None
    }
}
/// Generate glue code from .xml protocol file. The path is relative to your project root.
#[macro_export]
macro_rules! generate {
    ($path:literal) => {
        $crate::_private_scanner::generate!($path);
    };
}

/// Create a `&'static CStr` from a string literal. Panics at compile time if given string literal
/// contains null bytes.
#[macro_export]
macro_rules! cstr {
    ($str:literal) => {{
        const X: &'static ::std::ffi::CStr = $crate::_private_cstr(concat!($str, "\0"));
        X
    }};
}

// TODO: remove when MSRV is at least 1.72
#[doc(hidden)]
pub const fn _private_cstr(string: &str) -> &CStr {
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

/// Event callback context.
#[non_exhaustive]
pub struct EventCtx<'a, D, P: object::Proxy, T: ClientTransport = UnixStream> {
    pub conn: &'a mut Connection<D, T>,
    pub state: &'a mut D,
    pub proxy: P,
    pub event: P::Event,
}

impl<'a, D, P: object::Proxy, T: ClientTransport> fmt::Debug for EventCtx<'a, D, P, T>
where
    P: fmt::Debug,
    P::Event: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventCtx")
            .field("proxy", &self.proxy)
            .field("event", &self.event)
            .finish_non_exhaustive()
    }
}

#[doc(hidden)]
pub mod proxy {
    pub use crate::object::Proxy;
}
