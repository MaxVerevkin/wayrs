//! A simple Rust implementation of Wayland client library

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod global;
pub mod object;
pub mod protocol;

mod connection;
mod debug_message;

pub use connection::Connection;

#[doc(hidden)]
pub use wayrs_scanner as _private_scanner;

pub use wayrs_core as core;
pub use wayrs_core::transport::Transport;
pub use wayrs_core::{Fixed, IoMode};

use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::{env, fmt, io};

pub trait ClientTransport: Transport {
    type ConnectError: From<io::Error>;

    fn connect() -> Result<Self, Self::ConnectError>
    where
        Self: Sized;
}

/// An error that can occur while connecting to a Wayland socket.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// Either `$XDG_RUNTIME_DIR` or `$WAYLAND_DISPLAY` was not available.
    #[error("both $XDG_RUNTIME_DIR and $WAYLAND_DISPLAY must be set")]
    NotEnoughEnvVars,
    /// Some IO error.
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl ClientTransport for UnixStream {
    type ConnectError = ConnectError;

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
// TODO: remove when MSRV is 1.77
#[macro_export]
macro_rules! cstr {
    ($str:literal) => {{
        const X: &'static ::std::ffi::CStr =
            match ::std::ffi::CStr::from_bytes_with_nul(concat!($str, "\0").as_bytes()) {
                Ok(str) => str,
                Err(_) => panic!("string is not valind cstr"),
            };
        X
    }};
}

/// Event callback context.
#[non_exhaustive]
pub struct EventCtx<'a, D, P: object::Proxy, T = UnixStream> {
    pub conn: &'a mut Connection<D, T>,
    pub state: &'a mut D,
    pub proxy: P,
    pub event: P::Event,
}

impl<'a, D, P: object::Proxy, T> fmt::Debug for EventCtx<'a, D, P, T>
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
