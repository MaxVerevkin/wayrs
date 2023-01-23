//! Wayland connection

use std::collections::{HashMap, VecDeque};
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};

use crate::interface::Interface;
use crate::object::{Object, ObjectId};
use crate::protocol::wl_registry::GlobalArgs;
use crate::protocol::*;
use crate::proxy::Proxy;
use crate::socket::{BufferedSocket, SendMessageError};
use crate::wire::{ArgType, ArgValue, DebugMessage, Message};
use crate::{ConnectError, IoMode};

#[cfg(feature = "tokio")]
use tokio::io::unix::AsyncFd;

/// Wayland connection state.
///
/// This struct manages a buffered Wayland socket, keeps track of objects and request/event queues
/// and dispatches object events.
///
/// Set `WAYRS_DEBUG=1` environment variable to get debug messages.
pub struct Connection<D> {
    socket: BufferedSocket,

    reusable_ids: Vec<ObjectId>,
    last_id: ObjectId,

    event_queue: VecDeque<QueuedEvent>,
    requests_queue: VecDeque<Message>,
    break_dispatch: bool,

    display: WlDisplay,
    registry: WlRegistry,
    objects: HashMap<ObjectId, ObjectState<D>>,
    dead_objects: HashMap<ObjectId, Object>,

    #[cfg(feature = "tokio")]
    async_fd: Option<AsyncFd<RawFd>>,

    debug: bool,
}

enum QueuedEvent {
    DeleteId(ObjectId),
    Message(Message),
}

impl QueuedEvent {
    fn sender_id(&self) -> ObjectId {
        match self {
            Self::DeleteId(_) => ObjectId::DISPLAY,
            Self::Message(msg) => msg.header.object_id,
        }
    }
}

type GenericCallback<D> = Box<dyn FnMut(&mut Connection<D>, &mut D, Object, Message) + Send>;

struct ObjectState<D> {
    object: Object,
    cb: Option<GenericCallback<D>>,
}

impl<D> AsRawFd for Connection<D> {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

impl<D> Connection<D> {
    /// Connect to a Wayland socket at `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY` and create a registry.
    ///
    /// At the moment, only a single registry can be created. This might or might not change in the
    /// future, considering registries cannot be destroyed.
    pub fn connect() -> Result<Self, ConnectError> {
        let mut this = Self {
            socket: BufferedSocket::connect()?,

            reusable_ids: Vec::new(),
            last_id: ObjectId::NULL,

            event_queue: VecDeque::with_capacity(32),
            requests_queue: VecDeque::with_capacity(32),
            break_dispatch: false,

            display: WlDisplay::null(),
            registry: WlRegistry::null(),
            objects: HashMap::new(),
            dead_objects: HashMap::new(),

            #[cfg(feature = "tokio")]
            async_fd: None,

            debug: std::env::var("WAYRS_DEBUG").as_deref() == Ok("1"),
        };

        let display: WlDisplay = this.allocate_new_object(1);
        assert_eq!(display.id(), ObjectId::DISPLAY);
        this.display = display;

        this.registry = display.get_registry(&mut this);

        Ok(this)
    }

    /// Collect the initial set of advertised globals. This function must be called right after
    /// [`connect`](Self::connect) or not called at all.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use wayrs_client::connection::Connection;
    /// struct MyState;
    /// let mut conn = Connection::<MyState>::connect().unwrap();
    /// let globals = conn.blocking_collect_initial_globals().unwrap();
    /// ```
    pub fn blocking_collect_initial_globals(&mut self) -> io::Result<Vec<GlobalArgs>> {
        self.blocking_roundtrip()?;

        let mut globals = Vec::new();

        for event in self.event_queue.drain(..) {
            let QueuedEvent::Message(event) = event else { panic!() };
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

    /// Async version of [`blocking_collect_initial_globals`](Self::blocking_collect_initial_globals).
    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
    pub async fn async_collect_initial_globals(&mut self) -> io::Result<Vec<GlobalArgs>> {
        self.async_roundtrip().await?;

        let mut globals = Vec::new();

        for event in self.event_queue.drain(..) {
            let QueuedEvent::Message(event) = event else { panic!() };
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

    /// Get Wayland registry.
    ///
    /// At the moment, only a single registry can be created. This might or might not change in the
    /// future, considering registries cannot be destroyed.
    pub fn registry(&self) -> WlRegistry {
        self.registry
    }

    /// Set a callback for a given object.
    ///
    /// # Panics
    ///
    /// This method panics if current set of objects does not contain an object with id identical
    /// to `proxy.id()` or if internally stored object differs from `proxy`.
    ///
    /// Calling this function on a destroyed object will most likely panic, but this is not
    /// guarantied due to id-reuse.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use wayrs_client::connection::Connection;
    /// struct MyState;
    /// let mut conn = Connection::<MyState>::connect().unwrap();
    /// conn.set_callback_for(conn.registry(), |conn, state, registry, event| todo!());
    /// ```
    pub fn set_callback_for<
        P: Proxy,
        F: FnMut(&mut Connection<D>, &mut D, P, P::Event) + Send + 'static,
    >(
        &mut self,
        proxy: P,
        cb: F,
    ) {
        let object = self
            .objects
            .get_mut(&proxy.id())
            .expect("attempt to set a callback for non-existing or dead object");
        assert_eq!(object.object, proxy.into(), "object mismatch");
        object.cb = Some(Self::make_generic_cb(cb));
    }

    /// Perform a blocking roundtrip.
    ///
    /// This function flushes the buffer of pending requests. All received events during the
    /// roundtrip are queued.
    pub fn blocking_roundtrip(&mut self) -> io::Result<()> {
        let display = self.display;
        let sync_cb = display.sync(self);
        self.flush(IoMode::Blocking)?;

        loop {
            let event = self.recv_event(IoMode::Blocking)?;
            match event.sender_id() {
                id if id == sync_cb.id() => return Ok(()),
                _ => self.event_queue.push_back(event),
            }
        }
    }

    /// Async version of [`blocking_roundtrip`](Self::blocking_roundtrip).
    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
    pub async fn async_roundtrip(&mut self) -> io::Result<()> {
        let display = self.display;
        let sync_cb = display.sync(self);
        self.async_flush().await?;

        loop {
            let event = self.async_recv_event().await?;
            match event.sender_id() {
                id if id == sync_cb.id() => return Ok(()),
                _ => self.event_queue.push_back(event),
            }
        }
    }

    #[doc(hidden)]
    pub fn send_request(&mut self, iface: &'static Interface, request: Message) {
        if self.debug {
            let object = self
                .objects
                .get(&request.header.object_id)
                .expect("attempt to send request for non-existing or dead object")
                .object;
            eprintln!(
                "[wayrs]  -> {:?}",
                DebugMessage::new(&request, false, object)
            );
        }

        // Destroy object if request is destrctor
        if iface.requests[request.header.opcode as usize].is_destructor {
            let obj = self
                .objects
                .remove(&request.header.object_id)
                .expect("attempt to send request for non-existing or dead object")
                .object;
            self.dead_objects.insert(obj.id, obj);
        }

        // Queue request
        self.requests_queue.push_back(request);
    }

    fn recv_event(&mut self, mode: IoMode) -> io::Result<QueuedEvent> {
        let header = self.socket.peek_message_header(mode)?;

        let object = *self
            .objects
            .get(&header.object_id)
            .map(|o| &o.object)
            .or_else(|| self.dead_objects.get(&header.object_id))
            .expect("received event for non-existing object");

        let event = self.socket.recv_message(header, object.interface, mode)?;
        if self.debug {
            eprintln!("[wayrs] {:?}", DebugMessage::new(&event, true, object));
        }

        if event.header.object_id == ObjectId::DISPLAY {
            return match self.display.parse_event(event).unwrap() {
                // Catch protocol error as early as possible
                wl_display::Event::Error(err) => Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Error in object {} (code({})): {}",
                        err.object_id.0,
                        err.code,
                        err.message.to_string_lossy(),
                    ),
                )),
                wl_display::Event::DeleteId(id) => Ok(QueuedEvent::DeleteId(ObjectId(id))),
            };
        }

        // Allocate objects if necessary
        for (arg, arg_ty) in event
            .args
            .iter()
            .zip(object.interface.events[header.opcode as usize].signature)
        {
            if let ArgValue::NewId(id) = *arg {
                let ArgType::NewId(interface) = arg_ty else { panic!() };
                self.objects.insert(
                    id,
                    ObjectState {
                        object: Object {
                            id,
                            interface,
                            version: object.version,
                        },
                        cb: None,
                    },
                );
            }
        }

        Ok(QueuedEvent::Message(event))
    }

    #[cfg(feature = "tokio")]
    async fn async_recv_event(&mut self) -> io::Result<QueuedEvent> {
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

    /// Receive events from Wayland socket.
    ///
    /// If `mode` is [`Blocking`](IoMode::Blocking), this function will block the current thread
    /// until at least one event is read.
    ///
    /// If `mode` is [`NonBlocking`](IoMode::NonBlocking), this function will read form the socket
    /// until reading would block. If at least one event was received, `Ok` will be returned.
    /// Otherwise, [`WouldBlock`](io::ErrorKind::WouldBlock) will be propagated.
    ///
    /// Regular IO errors are propagated as usual.
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

    /// Async version of [`recv_events`](Self::recv_events).
    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
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

    /// Send the queue of pending request to the server.
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

    /// Async version of [`flush`](Self::flush).
    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
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

    /// Empty the queue of pending events, calling a callback (if set) for each event.
    pub fn dispatch_events(&mut self, state: &mut D) {
        self.break_dispatch = false;

        while let Some(event) = self.event_queue.pop_front() {
            match event {
                QueuedEvent::DeleteId(id) => {
                    assert!(!id.created_by_server());
                    self.objects.remove(&id);
                    self.dead_objects.remove(&id);
                    self.reusable_ids.push(id);
                }
                QueuedEvent::Message(event) => {
                    let Some(object) = self.objects.get_mut(&event.header.object_id)
                    else { continue };
                    // Removing the callback from the object to make borrow checker happy
                    let mut object_cb = object.cb.take();
                    let object = object.object;

                    match &mut object_cb {
                        Some(cb) => cb(self, state, object, event),
                        None => {
                            if self.debug {
                                eprintln!(
                                    "[wayrs] no callback for {}@{}",
                                    object.interface.name.to_string_lossy(),
                                    object.id.0,
                                );
                            }
                            continue;
                        }
                    }

                    // The object might have been destroyed
                    if let Some(object) = self.objects.get_mut(&object.id) {
                        // Callback might have been set again
                        if object.cb.is_none() {
                            object.cb = object_cb;
                        }
                    }

                    if self.break_dispatch {
                        break;
                    }
                }
            }
        }
    }

    /// Call this function from a callback to break the dispatch loop.
    ///
    /// This will cause [`dispatch_events`](Self::dispatch_events) to return. Events that go after
    /// current event are left in the queue.
    pub fn break_dispatch_loop(&mut self) {
        self.break_dispatch = true;
    }

    /// Allocate a new object. Returned object must be sent in a request as a "new_id" argument.
    pub fn allocate_new_object<P: Proxy>(&mut self, version: u32) -> P {
        let id = self.reusable_ids.pop().unwrap_or_else(|| {
            let id = self.last_id.next();
            assert!(!id.created_by_server());
            self.last_id = id;
            id
        });

        let object = Object {
            id,
            interface: P::interface(),
            version,
        };

        self.objects.insert(id, ObjectState { object, cb: None });

        object.try_into().unwrap()
    }

    /// Allocate a new object and set callback. Returned object must be sent in a request as a
    /// "new_id" argument.
    pub fn allocate_new_object_with_cb<
        P: Proxy,
        F: FnMut(&mut Connection<D>, &mut D, P, P::Event) + Send + 'static,
    >(
        &mut self,
        version: u32,
        cb: F,
    ) -> P {
        let id = self.reusable_ids.pop().unwrap_or_else(|| {
            let id = self.last_id.next();
            assert!(!id.created_by_server());
            self.last_id = id;
            id
        });

        let object = Object {
            id,
            interface: P::interface(),
            version,
        };

        self.objects.insert(
            id,
            ObjectState {
                object,
                cb: Some(Self::make_generic_cb(cb)),
            },
        );

        object.try_into().unwrap()
    }

    fn make_generic_cb<
        P: Proxy,
        F: FnMut(&mut Connection<D>, &mut D, P, P::Event) + Send + 'static,
    >(
        mut cb: F,
    ) -> GenericCallback<D> {
        // Note: if `F` does not capture anything, this `Box::new` will not allocate.
        Box::new(move |conn, state, object, event| {
            let proxy: P = object.try_into().unwrap();
            let event = proxy.parse_event(event).unwrap();
            cb(conn, state, proxy, event);
        })
    }
}

#[cfg(feature = "tokio")]
impl<D> Drop for Connection<D> {
    fn drop(&mut self) {
        // Drop AsyncFd before BufferedSocket
        if let Some(async_fd) = self.async_fd.take() {
            let _ = async_fd.into_inner();
        }
    }
}
