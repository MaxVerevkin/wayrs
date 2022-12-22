use std::borrow::Cow;
use std::ffi::CStr;
use std::os::unix::io::OwnedFd;

use crate::{
    interface::Interface,
    object::{Object, ObjectId},
};

#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    pub object_id: ObjectId,
    pub size: u16,
    pub opcode: u16,
}

#[derive(Debug)]
pub struct Message {
    pub header: MessageHeader,
    pub args: Vec<ArgValue>,
}

#[derive(Debug)]
pub enum ArgType {
    Int,
    Uint,
    Fixed,
    Object,
    NewId(&'static Interface),
    AnyNewId,
    String,
    Array,
    Fd,
    Enum,
}

#[derive(Debug)]
pub enum ArgValue {
    Int(i32),
    Uint(u32),
    Fixed(Fixed),
    Object(ObjectId),
    NewId(Object),
    String(Cow<'static, CStr>),
    Array(Vec<u8>),
    Fd(OwnedFd),
    Enum(u32),
}

impl ArgValue {
    pub fn size(&self) -> u16 {
        match self {
            Self::Int(_)
            | Self::Uint(_)
            | Self::Fixed(_)
            | Self::Object(_)
            | Self::NewId(_)
            | Self::Enum(_) => 4,
            Self::String(string) => {
                let len = string.to_bytes_with_nul().len() as u16;
                let padding = (4 - (len % 4)) % 4;
                4 + len + padding
            }
            Self::Array(array) => {
                let len = array.len() as u16;
                let padding = (4 - (len % 4)) % 4;
                4 + len + padding
            }
            Self::Fd(_) => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixed(pub i32);

impl Fixed {
    pub fn as_f64(self) -> f64 {
        self.0 as f64 / 256.0
    }

    pub fn from_i32(val: i32) -> Self {
        Self(val * 256)
    }
}
