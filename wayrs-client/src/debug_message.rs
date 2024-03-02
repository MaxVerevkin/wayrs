use std::fmt::{self, Debug, Formatter};
use std::os::fd::AsRawFd;

use wayrs_core::{ArgType, ArgValue, Message, ObjectId};

use crate::object::Object;

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
                ArgValue::NewId(id) => {
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
                ArgValue::AnyNewId(iface, version, id) => write!(
                    f,
                    "new id {}@{}v{version}",
                    id.as_u32(),
                    iface.to_string_lossy()
                )?,
                ArgValue::String(x) | ArgValue::OptString(Some(x)) => write!(f, "{x:?}")?,
                ArgValue::Array(_) => write!(f, "<array>")?,
                ArgValue::Fd(x) => write!(f, "fd {}", x.as_raw_fd())?,
            }
        }

        write!(f, ")")
    }
}
