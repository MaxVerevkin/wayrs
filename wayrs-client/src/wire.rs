use std::ffi::CString;
use std::fmt::{self, Debug, Formatter};
use std::os::fd::{AsRawFd, OwnedFd};

use crate::interface::Interface;
use crate::object::{Object, ObjectId};

#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    pub object_id: ObjectId,
    pub size: u16,
    pub opcode: u16,
}

impl MessageHeader {
    pub const fn size() -> u16 {
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

    NewIdRequest(ObjectId),
    AnyNewIdRequest(Object),
    NewIdEvent(Object),

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
            | Self::OptObject(_)
            | Self::NewIdRequest(_)
            | Self::NewIdEvent(_)
            | Self::OptString(None) => 4,
            Self::AnyNewIdRequest(object) => {
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

/// Signed 24.8 decimal number
#[derive(Clone, Copy, PartialEq, Eq)]
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

    pub fn as_f32(self) -> f32 {
        self.0 as f32 / 256.0
    }

    pub fn as_int(self) -> i32 {
        self.0 / 256
    }
}

impl Debug for Fixed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.as_f64().fmt(f)
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
                write!(f, ", ")?;
            }
            match arg {
                ArgValue::Int(x) => write!(f, "{x}")?,
                ArgValue::Uint(x) => write!(f, "{x}")?,
                ArgValue::Object(ObjectId(x)) | ArgValue::OptObject(Some(ObjectId(x))) => {
                    write!(f, "{x}")?
                }
                ArgValue::OptObject(None) | ArgValue::OptString(None) => write!(f, "null")?,
                ArgValue::Fixed(x) => write!(f, "{}", x.as_f64())?,
                ArgValue::NewIdRequest(id) => {
                    let ArgType::NewId(new_id_iface) = &msg_desc.signature[arg_i] else {
                        panic!("signature mismatch")
                    };
                    write!(
                        f,
                        "new id {}@{}",
                        new_id_iface.name.to_string_lossy(),
                        id.as_u32()
                    )?
                }
                ArgValue::AnyNewIdRequest(x) | ArgValue::NewIdEvent(x) => {
                    write!(f, "new id {x:?}")?
                }
                ArgValue::String(x) | ArgValue::OptString(Some(x)) => write!(f, "{x:?}")?,
                ArgValue::Array(_) => write!(f, "<array>")?,
                ArgValue::Fd(x) => write!(f, "fd {}", x.as_raw_fd())?,
            }
        }

        write!(f, ")")
    }
}
