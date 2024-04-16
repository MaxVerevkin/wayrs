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

use std::fmt;

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
pub struct EventCtx<'a, D, P: object::Proxy> {
    pub conn: &'a mut Connection<D>,
    pub state: &'a mut D,
    pub proxy: P,
    pub event: P::Event,
}

impl<'a, D, P: object::Proxy> fmt::Debug for EventCtx<'a, D, P>
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
pub mod interface {
    pub use crate::core::{Interface, MessageDesc};
}
#[doc(hidden)]
pub mod proxy {
    pub use crate::object::{BadMessage, Proxy, WrongObject};
}
#[doc(hidden)]
pub mod wire {
    pub use crate::core::{ArgType, ArgValue, Fixed, Message, MessageHeader};
}
