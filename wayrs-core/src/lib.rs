//! Core Wayland functionality
//!
//! It can be used on both client and server side.

use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU32;
use std::os::fd::OwnedFd;

mod ring_buffer;
pub mod transport;

/// The "mode" of an IO operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoMode {
    /// Blocking.
    ///
    /// The function call may block, but it will never return [WouldBlock](std::io::ErrorKind::WouldBlock)
    /// error.
    Blocking,
    /// Non-blocking.
    ///
    /// The function call will not block on IO operations. [WouldBlock](std::io::ErrorKind::WouldBlock)
    /// error is returned if the operation cannot be completed immediately.
    NonBlocking,
}

/// A Wayland object ID.
///
/// Uniquely identifies an object at each point of time. Note that an ID may have a limited
/// lifetime. Also an ID which once pointed to a certain object, may point to a different object in
/// the future, due to ID reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub NonZeroU32);

impl ObjectId {
    pub const DISPLAY: Self = Self(unsafe { NonZeroU32::new_unchecked(1) });
    pub const MAX_CLIENT: Self = Self(unsafe { NonZeroU32::new_unchecked(0xFEFFFFFF) });
    pub const MIN_SERVER: Self = Self(unsafe { NonZeroU32::new_unchecked(0xFF000000) });

    /// Returns the numeric representation of the ID
    pub fn as_u32(self) -> u32 {
        self.0.get()
    }

    /// Whether the object with this ID was created by the server
    pub fn created_by_server(self) -> bool {
        self >= Self::MIN_SERVER
    }

    /// Whether the object with this ID was created by the client
    pub fn created_by_client(self) -> bool {
        self <= Self::MAX_CLIENT
    }
}

/// A header of a Wayland message
#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    /// The ID of the associated object
    pub object_id: ObjectId,
    /// Size of the message in bytes, including the header
    pub size: u16,
    /// The opcode of the message
    pub opcode: u16,
}

impl MessageHeader {
    /// The size of the header in bytes
    pub const SIZE: usize = 8;
}

/// A Wayland message
#[derive(Debug)]
pub struct Message {
    pub header: MessageHeader,
    pub args: Vec<ArgValue>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ArgType {
    Int,
    Uint,
    Fixed,
    Object,
    OptObject,
    NewId(&'static Interface),
    AnyNewId,
    String,
    OptString,
    Array,
    Fd,
}

#[derive(Debug)]
pub enum ArgValue {
    Int(i32),
    Uint(u32),
    Fixed(Fixed),
    Object(ObjectId),
    OptObject(Option<ObjectId>),
    NewId(ObjectId),
    AnyNewId(Cow<'static, CStr>, u32, ObjectId),
    String(CString),
    OptString(Option<CString>),
    Array(Vec<u8>),
    Fd(OwnedFd),
}

impl ArgValue {
    /// The size of the argument in bytes.
    pub fn size(&self) -> usize {
        match self {
            Self::Int(_)
            | Self::Uint(_)
            | Self::Fixed(_)
            | Self::Object(_)
            | Self::OptObject(_)
            | Self::NewId(_)
            | Self::OptString(None) => 4,
            Self::AnyNewId(iface, _version, _id) => {
                iface.to_bytes_with_nul().len().next_multiple_of(4) + 12
            }
            Self::String(string) | Self::OptString(Some(string)) => {
                string.to_bytes_with_nul().len().next_multiple_of(4) + 4
            }
            Self::Array(array) => array.len().next_multiple_of(4) + 4,
            Self::Fd(_) => 0,
        }
    }
}

/// Signed 24.8 decimal number
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fixed(pub i32);

impl From<i32> for Fixed {
    fn from(value: i32) -> Self {
        Self(value * 256)
    }
}

impl From<u32> for Fixed {
    fn from(value: u32) -> Self {
        Self(value as i32 * 256)
    }
}

impl From<f32> for Fixed {
    fn from(value: f32) -> Self {
        Self((value * 256.0) as i32)
    }
}

impl From<f64> for Fixed {
    fn from(value: f64) -> Self {
        Self((value * 256.0) as i32)
    }
}

impl Fixed {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(256);
    pub const MINUS_ONE: Self = Self(-256);

    pub fn as_f64(self) -> f64 {
        self.0 as f64 / 256.0
    }

    pub fn as_f32(self) -> f32 {
        self.0 as f32 / 256.0
    }

    pub fn as_int(self) -> i32 {
        self.0 / 256
    }

    pub fn is_int(self) -> bool {
        self.0 & 255 == 0
    }
}

impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_f64().fmt(f)
    }
}

/// A Wayland interface, usually generated from the XML files
///
/// `PartialEq` and `Hash` implementations are delegated to the `name` field for performance reasons.
pub struct Interface {
    pub name: &'static CStr,
    pub version: u32,
    pub events: &'static [MessageDesc],
    pub requests: &'static [MessageDesc],
}

/// A "description" of a single Wayland event or request
#[derive(Debug, Clone, Copy)]
pub struct MessageDesc {
    pub name: &'static str,
    pub is_destructor: bool,
    pub signature: &'static [ArgType],
}

impl PartialEq for &'static Interface {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for &'static Interface {}

impl Hash for &'static Interface {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl fmt::Debug for Interface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Interface").field(&self.name).finish()
    }
}

/// A pool of resources reusable between messages
#[derive(Default)]
pub struct MessageBuffersPool {
    pool: Vec<Vec<ArgValue>>,
}

impl MessageBuffersPool {
    pub fn reuse_args(&mut self, mut buf: Vec<ArgValue>) {
        buf.clear();
        self.pool.push(buf);
    }

    pub fn get_args(&mut self) -> Vec<ArgValue> {
        self.pool.pop().unwrap_or_default()
    }
}
