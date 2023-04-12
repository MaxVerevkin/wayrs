use std::ffi::CString;
use std::fmt::{self, Debug, Formatter};
use std::os::unix::io::{AsRawFd, OwnedFd};

use crate::interface::Interface;
use crate::object::{Object, ObjectId};

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
    NewId(ObjectId),
    AnyNewId(Object),
    String(CString),
    OptString(Option<CString>),
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
            Self::Int(_)
            | Self::Uint(_)
            | Self::Fixed(_)
            | Self::Object(_)
            | Self::NewId(_)
            | Self::OptString(None) => 4,
            Self::AnyNewId(object) => {
                len_with_padding(object.interface.name.to_bytes_with_nul().len()) + 8
            }
            Self::String(string) | Self::OptString(Some(string)) => {
                len_with_padding(string.to_bytes_with_nul().len())
            }
            Self::Array(array) => len_with_padding(array.len()),
            Self::Fd(_) => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl Fixed {
    pub fn as_f64(self) -> f64 {
        self.0 as f64 / 256.0
    }
}

pub(crate) struct DebugMessage<'a> {
    message: &'a Message,
    is_event: bool,
    object: Object,
}

impl<'a> DebugMessage<'a> {
    pub(crate) fn new(message: &'a Message, is_event: bool, object: Object) -> Self {
        Self {
            message,
            is_event,
            object,
        }
    }
}

impl Debug for DebugMessage<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let msg_desc = if self.is_event {
            self.object.interface.events[self.message.header.opcode as usize]
        } else {
            self.object.interface.requests[self.message.header.opcode as usize]
        };

        write!(f, "{:?}.{}(", self.object, msg_desc.name,)?;

        for (arg_i, arg) in self.message.args.iter().enumerate() {
            if arg_i != 0 {
                write!(f, ",")?;
            }
            match arg {
                ArgValue::Int(x) => write!(f, "{x}")?,
                ArgValue::Uint(x) | ArgValue::Object(ObjectId(x)) => write!(f, "{x}")?,
                ArgValue::Fixed(x) => write!(f, "{}", x.as_f64())?,
                ArgValue::NewId(ObjectId(id)) => {
                    let ArgType::NewId(new_id_iface) = &msg_desc.signature[arg_i]
                    else { panic!("signature mismatch") };
                    write!(f, "{}@{id}", new_id_iface.name.to_string_lossy())?
                }
                ArgValue::AnyNewId(x) => write!(f, "{x:?}")?,
                ArgValue::String(x) | ArgValue::OptString(Some(x)) => write!(f, "{x:?}")?,
                ArgValue::OptString(None) => write!(f, "null")?,
                ArgValue::Array(_) => write!(f, "<array>")?,
                ArgValue::Fd(x) => write!(f, "{}", x.as_raw_fd())?,
            }
        }

        write!(f, ")")
    }
}
