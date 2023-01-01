use std::ffi::CString;
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

impl MessageHeader {
    pub fn size() -> u16 {
        8
    }
}

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
    NewId(&'static Interface),
    AnyNewId,
    String,
    Array,
    Fd,
}

#[derive(Debug)]
pub enum ArgValue {
    Int(i32),
    Uint(u32),
    Fixed(Fixed),
    Object(ObjectId),
    NewId(ObjectId),
    AnyNewId(Object),
    String(CString),
    Array(Vec<u8>),
    Fd(OwnedFd),
}

impl ArgValue {
    pub fn size(&self) -> u16 {
        fn len_with_padding(len: usize) -> u16 {
            let padding = (4 - (len % 4)) % 4;
            (4 + len + padding) as u16
        }

        match self {
            Self::Int(_) | Self::Uint(_) | Self::Fixed(_) | Self::Object(_) | Self::NewId(_) => 4,
            Self::AnyNewId(object) => {
                len_with_padding(object.interface.name.to_bytes_with_nul().len()) + 8
            }
            Self::String(string) => len_with_padding(string.to_bytes_with_nul().len()),
            Self::Array(array) => len_with_padding(array.len()),
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
