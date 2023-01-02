use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};

use crate::interface::{Interface, MessageDesc};
use crate::object::{Object, ObjectId};
use crate::protocol::wl_registry::GlobalArgs;
use crate::protocol::*;
use crate::proxy::{make_callback, Dispatch, Dispatcher, EventCallback, Proxy};
use crate::socket::{BufferedSocket, IoMode, SendMessageError};
use crate::wire::{ArgType, ArgValue, Message, MessageHeader};
use crate::ConnectError;

#[cfg(feature = "tokio")]
use tokio::io::unix::AsyncFd;

pub struct Connection<D: Dispatcher> {
    socket: BufferedSocket,

    reusable_ids: Vec<ObjectId>,
    last_id: ObjectId,

    pub(crate) event_queue: VecDeque<Message>,
    requests_queue: VecDeque<Message>,
    pub(crate) break_dispatch: bool,

    registry: WlRegistry,
    objects: HashMap<ObjectId, Object>,
    dead_objects: HashMap<ObjectId, Object>,
    pub(crate) callbacks: HashMap<&'static Interface, EventCallback<D>>,

    #[cfg(feature = "tokio")]
    pub(crate) async_fd: Option<AsyncFd<RawFd>>,
}

impl<D: Dispatcher> AsRawFd for Connection<D> {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

impl<D: Dispatcher> Connection<D> {
    pub fn connect() -> Result<Self, ConnectError>
    where
        D: Dispatch<WlRegistry>,
    {
        let mut this = Self {
            socket: BufferedSocket::connect()?,

            reusable_ids: Vec::new(),
            last_id: ObjectId::DISPLAY,

            event_queue: VecDeque::with_capacity(32),
            requests_queue: VecDeque::with_capacity(32),
            break_dispatch: false,

            registry: WlRegistry::null(),
            objects: HashMap::new(),
            dead_objects: HashMap::new(),
            callbacks: HashMap::new(),

            #[cfg(feature = "tokio")]
            async_fd: None,
        };

        let registry = WlDisplay.get_registry(&mut this);
        this.registry = registry;

        this.set_callback::<WlRegistry>();

        Ok(this)
    }

    pub fn blocking_collect_initial_globals(&mut self) -> io::Result<Vec<GlobalArgs>> {
        self.blocking_roundtrip()?;

        let mut globals = Vec::new();

        for event in self.event_queue.drain(..) {
            assert_eq!(event.header.object_id, self.registry.id());
            match self.registry.parse_event(event).unwrap() {
                wl_registry::Event::Global(global) => globals.push(global),
                wl_registry::Event::GlobalRemove(name) => {
                    globals.retain(|g| g.name != name);
                }
            }
        }

        Ok(globals)
    }

    #[cfg(feature = "tokio")]
    pub async fn async_collect_initial_globals(&mut self) -> io::Result<Vec<GlobalArgs>> {
        self.async_roundtrip().await?;

        let mut globals = Vec::new();

        for event in self.event_queue.drain(..) {
            assert_eq!(event.header.object_id, self.registry.id());
            match self.registry.parse_event(event).unwrap() {
                wl_registry::Event::Global(global) => globals.push(global),
                wl_registry::Event::GlobalRemove(name) => {
                    globals.retain(|g| g.name != name);
                }
            }
        }

        Ok(globals)
    }

    pub fn registry(&self) -> WlRegistry {
        self.registry
    }

    pub fn set_callback<P: Proxy>(&mut self)
    where
        D: Dispatch<P>,
    {
        self.callbacks.insert(P::interface(), make_callback::<P, D>);
    }

    pub fn blocking_roundtrip(&mut self) -> io::Result<()> {
        let sync_cb = WlDisplay.sync(self);
        self.flush(IoMode::Blocking)?;

        loop {
            let event = self.recv_event(IoMode::Blocking)?;
            match event.header.object_id {
                id if id == sync_cb.id() => return Ok(()),
                _ => self.event_queue.push_back(event),
            }
        }
    }

    #[cfg(feature = "tokio")]
    pub async fn async_roundtrip(&mut self) -> io::Result<()> {
        let sync_cb = WlDisplay.sync(self);
        self.async_flush().await?;

        loop {
            let event = self.async_recv_event().await?;
            match event.header.object_id {
                id if id == sync_cb.id() => return Ok(()),
                _ => self.event_queue.push_back(event),
            }
        }
    }

    pub fn send_request(&mut self, iface: &'static Interface, request: Message) {
        // Destroy object if request is destrctor
        if iface.requests[request.header.opcode as usize].is_destructor {
            let obj = self.objects.remove(&request.header.object_id).unwrap();
            self.dead_objects.insert(obj.id, obj);
        }

        // Queue request
        self.requests_queue.push_back(request);
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

        let event = self.socket.recv_message(header, interface, mode)?;

        if event.header.object_id == ObjectId::DISPLAY {
            let display_event = WlDisplay::parse_event(&event);
            if let WlDisplayEvent::Error {
                object_id,
                code,
                message,
            } = display_event
            {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Error in object {} (code({code})): {message:?}",
                        object_id.0
                    ),
                ));
            }
        }

        // Allocate objects if necessary
        for (arg, arg_ty) in event
            .args
            .iter()
            .zip(interface.events[header.opcode as usize].signature)
        {
            if let ArgValue::NewId(id) = *arg {
                let ArgType::NewId(interface) = arg_ty else { panic!() };
                self.objects.insert(
                    id,
                    Object {
                        id,
                        interface,
                        version,
                    },
                );
            }
        }

        Ok(event)
    }

    #[cfg(feature = "tokio")]
    pub async fn async_recv_event(&mut self) -> io::Result<Message> {
        let mut async_fd = match self.async_fd.take() {
            Some(fd) => fd,
            None => AsyncFd::new(self.as_raw_fd())?,
        };

        loop {
            let mut fd_guard = async_fd.readable_mut().await?;
            match fd_guard.try_io(|_| self.recv_event(IoMode::NonBlocking)) {
                Ok(result) => {
                    self.async_fd = Some(async_fd);
                    return result;
                }
                Err(_would_block) => continue,
            }
        }
    }

    pub fn recv_events(&mut self, mut mode: IoMode) -> io::Result<()> {
        let mut at_least_one = false;

        loop {
            let msg = match self.recv_event(mode) {
                Ok(msg) => msg,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock && at_least_one => return Ok(()),
                Err(e) => return Err(e),
            };

            at_least_one = true;
            mode = IoMode::NonBlocking;
            self.event_queue.push_back(msg);
        }
    }

    #[cfg(feature = "tokio")]
    pub async fn async_recv_events(&mut self) -> io::Result<()> {
        let event = self.async_recv_event().await?;
        self.event_queue.push_back(event);
        loop {
            match self.recv_event(IoMode::NonBlocking) {
                Ok(msg) => self.event_queue.push_back(msg),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) => return Err(e),
            };
        }
    }

    pub fn flush(&mut self, mode: IoMode) -> io::Result<()> {
        // Send pending messages
        while let Some(msg) = self.requests_queue.pop_front() {
            if let Err(SendMessageError { msg, err }) = self.socket.write_message(msg, mode) {
                self.requests_queue.push_front(msg);
                return Err(err);
            }
        }

        // Flush socket
        self.socket.flush(mode)
    }

    #[cfg(feature = "tokio")]
    pub async fn async_flush(&mut self) -> io::Result<()> {
        let mut async_fd = match self.async_fd.take() {
            Some(fd) => fd,
            None => AsyncFd::new(self.as_raw_fd())?,
        };

        loop {
            let mut fd_guard = async_fd.writable_mut().await?;
            match fd_guard.try_io(|_| self.flush(IoMode::NonBlocking)) {
                Ok(result) => {
                    self.async_fd = Some(async_fd);
                    return result;
                }
                Err(_would_block) => continue,
            }
        }
    }

    pub fn dispatch_events(&mut self, state: &mut D) -> Result<(), D::Error> {
        while let Some(event) = self.event_queue.pop_front() {
            if event.header.object_id == ObjectId::DISPLAY {
                self.process_dispay_event(&event).unwrap();
                continue;
            }

            let Some(object) = self
                .get_object(event.header.object_id)
            else { continue };

            let Some(cb) = self
                .callbacks
                .get(&object.interface)
            else { panic!("dispatch callback for {:?} not found", object.interface.name) };

            cb(self, state, object, event)?;

            if self.break_dispatch {
                self.break_dispatch = false;
                break;
            }
        }
        Ok(())
    }

    pub fn break_dispatch_loop(&mut self) {
        self.break_dispatch = true;
    }

    pub fn get_object(&self, id: ObjectId) -> Option<Object> {
        self.objects.get(&id).copied()
    }

    /// Allocate a new object. Returned object must be sent in a request as a "new_id" argument.
    pub fn allocate_new_object<P: Proxy>(&mut self, version: u32) -> P {
        let id = self.reusable_ids.pop().unwrap_or_else(|| {
            let id = self.last_id.next();
            assert!(!id.created_by_server());
            self.last_id = id;
            id
        });

        let obj = Object {
            id,
            interface: P::interface(),
            version,
        };

        self.objects.insert(id, obj);

        obj.try_into().unwrap()
    }

    pub(crate) fn process_dispay_event(&mut self, msg: &Message) -> io::Result<()> {
        match WlDisplay::parse_event(msg) {
            WlDisplayEvent::Error { .. } => {
                unreachable!()
            }
            WlDisplayEvent::DeleteId { id } => {
                let id = ObjectId(id);
                assert!(!id.created_by_server());
                self.objects.remove(&id);
                self.dead_objects.remove(&id);
                self.reusable_ids.push(id);
                Ok(())
            }
        }
    }
}

#[cfg(feature = "tokio")]
impl<D: Dispatcher> Drop for Connection<D> {
    fn drop(&mut self) {
        // Drop AsyncFd before BufferedSocket
        if let Some(async_fd) = self.async_fd.take() {
            let _ = async_fd.into_inner();
        }
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
    pub(crate) fn sync(&self, conn: &mut Connection<impl Dispatcher>) -> WlCallback {
        let new_object = conn.allocate_new_object::<WlCallback>(1);
        conn.send_request(
            WL_DISPLAY_INTERFACE,
            Message {
                header: MessageHeader {
                    object_id: ObjectId::DISPLAY,
                    size: 0,
                    opcode: 0,
                },
                args: vec![ArgValue::NewId(new_object.id())],
            },
        );
        new_object
    }

    pub(crate) fn get_registry(&self, conn: &mut Connection<impl Dispatcher>) -> WlRegistry {
        let new_object = conn.allocate_new_object::<WlRegistry>(1);
        conn.send_request(
            WL_DISPLAY_INTERFACE,
            Message {
                header: MessageHeader {
                    object_id: ObjectId::DISPLAY,
                    size: 0,
                    opcode: 1,
                },
                args: vec![ArgValue::NewId(new_object.id())],
            },
        );
        new_object
    }

    pub(crate) fn parse_event(msg: &Message) -> WlDisplayEvent {
        match msg.header.opcode {
            0 => {
                let mut args = msg.args.iter();
                let Some(ArgValue::Object(object_id)) = args.next() else { unreachable!() };
                let Some(ArgValue::Uint(code)) = args.next() else { unreachable!() };
                let Some(ArgValue::String(message)) = args.next() else { unreachable!() };
                WlDisplayEvent::Error {
                    object_id: *object_id,
                    code: *code,
                    message: message.clone(),
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
                signature: &[crate::wire::ArgType::NewId(wl_callback::INTERFACE)],
            },
            MessageDesc {
                name: "get_registry",
                is_destructor: false,
                signature: &[crate::wire::ArgType::NewId(wl_registry::INTERFACE)],
            },
        ],
    };
