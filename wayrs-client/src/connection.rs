use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};

use crate::interface::{Interface, MessageDesc};
use crate::protocol::wl_callback::WL_CALLBACK_INTERFACE;
use crate::protocol::wl_registry::WL_REGISTRY_INTERFACE;

use crate::object::{Object, ObjectId};
use crate::protocol::wl_callback::WlCallback;
use crate::protocol::wl_registry::WlRegistry;
use crate::socket::{BufferedSocket, IoMode};
use crate::wire::{ArgValue, Message, MessageHeader};
use crate::ConnectError;

pub struct Connection {
    socket: BufferedSocket,
    objects: HashMap<ObjectId, Object>,
    dead_objects: HashMap<ObjectId, Object>,
    reusable_ids: Vec<ObjectId>,
    last_id: ObjectId,
    requests_queue: VecDeque<Message>,
}

impl AsRawFd for Connection {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

impl Connection {
    pub fn connect() -> Result<Self, ConnectError> {
        Ok(Self {
            socket: BufferedSocket::connect()?,
            objects: HashMap::new(),
            dead_objects: HashMap::new(),
            reusable_ids: Vec::new(),
            last_id: ObjectId::DISPLAY,
            requests_queue: VecDeque::with_capacity(32),
        })
    }

    pub fn send_request(
        &mut self,
        iface: &'static Interface,
        mut request: Message,
    ) -> Option<Object> {
        // Allocate object if necessary
        let mut new_object = None;
        for arg in &mut request.args {
            if let ArgValue::NewId(new_obj) = arg {
                new_obj.id = self.allocate_object_id();
                self.objects.insert(new_obj.id, *new_obj);
                assert!(new_object.is_none());
                new_object = Some(*new_obj);
            }
        }

        // Destroy object if request is destrctor
        if iface.requests[request.header.opcode as usize].is_destructor {
            let obj = self.objects.remove(&request.header.object_id).unwrap();
            self.dead_objects.insert(obj.id, obj);
        }

        // Queue request
        self.requests_queue.push_back(request);

        new_object
    }

    pub fn recv_event(&mut self, mode: IoMode) -> io::Result<Message> {
        let header = self.socket.peek_message_header(mode)?;

        let (interface, version) = if header.object_id == ObjectId::DISPLAY {
            (WL_DISPLAY_INTERFACE, 1)
        } else {
            let object = self
                .objects
                .get(&header.object_id)
                .or_else(|| self.dead_objects.get(&header.object_id))
                .expect("received event for non-existing object");
            (object.interface, object.version)
        };

        let event = self.socket.recv_message(header, interface, version, mode)?;

        // Allocate objects if necessary
        for arg in &event.args {
            if let ArgValue::NewId(new_obj) = arg {
                self.objects.insert(new_obj.id, *new_obj);
            }
        }

        Ok(event)
    }

    pub fn flush(&mut self, mode: IoMode) -> io::Result<()> {
        // Send pending messages
        while let Some(msg) = self.requests_queue.pop_front() {
            if let Err(err) = self.socket.write_message(msg, mode) {
                self.requests_queue.push_front(err.msg);
                return Err(err.err);
            }
        }

        // Flush socket
        self.socket.flush(mode)
    }

    pub fn get_object(&self, id: ObjectId) -> Option<Object> {
        self.objects.get(&id).copied()
    }
}

impl Connection {
    pub(crate) fn process_dispay_event(&mut self, msg: Message) {
        match WlDisplay::parse_event(msg) {
            WlDisplayEvent::Error {
                object_id,
                code,
                message,
            } => {
                panic!(
                    "Error in object {} (code {code}): {:#?}",
                    object_id.0, message
                );
            }
            WlDisplayEvent::DeleteId { id } => {
                let id = ObjectId(id);
                assert!(!id.created_by_server());
                self.objects.remove(&id);
                self.dead_objects.remove(&id);
                self.reusable_ids.push(id);
            }
        }
    }

    fn allocate_object_id(&mut self) -> ObjectId {
        self.reusable_ids.pop().unwrap_or_else(|| {
            let id = self.last_id.next();
            assert!(!id.created_by_server());
            self.last_id = id;
            id
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WlDisplay;

pub(crate) enum WlDisplayEvent {
    Error {
        object_id: ObjectId,
        code: u32,
        message: CString,
    },
    DeleteId {
        id: u32,
    },
}

impl WlDisplay {
    pub(crate) fn sync(&self, conn: &mut Connection) -> WlCallback {
        let new_object = conn.send_request(
            WL_DISPLAY_INTERFACE,
            Message {
                header: MessageHeader {
                    object_id: ObjectId::DISPLAY,
                    size: 0,
                    opcode: 0,
                },
                args: vec![ArgValue::NewId(Object {
                    id: ObjectId::NULL,
                    interface: WL_CALLBACK_INTERFACE,
                    version: 1,
                })],
            },
        );

        new_object.unwrap().try_into().unwrap()
    }

    pub(crate) fn get_registry(&self, conn: &mut Connection) -> WlRegistry {
        let new_object = conn.send_request(
            WL_DISPLAY_INTERFACE,
            Message {
                header: MessageHeader {
                    object_id: ObjectId::DISPLAY,
                    size: 0,
                    opcode: 1,
                },
                args: vec![ArgValue::NewId(Object {
                    id: ObjectId::NULL,
                    interface: WL_REGISTRY_INTERFACE,
                    version: 1,
                })],
            },
        );

        new_object.unwrap().try_into().unwrap()
    }

    pub(crate) fn parse_event(msg: Message) -> WlDisplayEvent {
        match msg.header.opcode {
            0 => {
                let mut args = msg.args.into_iter();
                let Some(ArgValue::Object(object_id)) = args.next() else { unreachable!() };
                let Some(ArgValue::Uint(code)) = args.next() else { unreachable!() };
                let Some(ArgValue::String(message)) = args.next() else { unreachable!() };
                WlDisplayEvent::Error {
                    object_id,
                    code,
                    message: message.into_owned(),
                }
            }
            1 => {
                let ArgValue::Uint(id) = msg.args[0] else { unreachable!() };
                WlDisplayEvent::DeleteId { id }
            }
            _ => unreachable!(),
        }
    }
}

pub(crate) static WL_DISPLAY_INTERFACE: &crate::interface::Interface =
    &crate::interface::Interface {
        name: crate::cstr!("wl_display"),
        version: 1u32,
        events: &[
            MessageDesc {
                name: "error",
                is_destructor: false,
                signature: &[
                    crate::wire::ArgType::Object,
                    crate::wire::ArgType::Uint,
                    crate::wire::ArgType::String,
                ],
            },
            MessageDesc {
                name: "delete_id",
                is_destructor: false,
                signature: &[crate::wire::ArgType::Uint],
            },
        ],
        requests: &[
            MessageDesc {
                name: "sync",
                is_destructor: false,
                signature: &[crate::wire::ArgType::NewId(WL_CALLBACK_INTERFACE)],
            },
            MessageDesc {
                name: "get_registry",
                is_destructor: false,
                signature: &[crate::wire::ArgType::Uint],
            },
        ],
    };
