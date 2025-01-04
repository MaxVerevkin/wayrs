//! Wayland connection

use std::collections::VecDeque;
use std::env;
use std::fmt;
use std::io;
use std::num::NonZeroU32;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use crate::debug_message::DebugMessage;
use crate::global::BindError;
use crate::global::GlobalExt;
use crate::global::VersionBounds;
use crate::object::{Object, ObjectManager, Proxy};
use crate::protocol::wl_registry::GlobalArgs;
use crate::protocol::*;
use crate::EventCtx;

use wayrs_core::transport::{BufferedSocket, PeekHeaderError, RecvMessageError, SendMessageError};
use wayrs_core::{ArgType, ArgValue, Interface, IoMode, Message, MessageBuffersPool, ObjectId};

#[cfg(feature = "tokio")]
use tokio::io::unix::AsyncFd;

/// An error that can occur while connecting to a Wayland socket.
#[derive(Debug)]
pub enum ConnectError {
    /// Either `$XDG_RUNTIME_DIR` or `$WAYLAND_DISPLAY` was not available.
    NotEnoughEnvVars,
    /// Some IO error.
    Io(io::Error),
}

impl std::error::Error for ConnectError {}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnoughEnvVars => {
                f.write_str("both $XDG_RUNTIME_DIR and $WAYLAND_DISPLAY must be set")
            }
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl From<io::Error> for ConnectError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Wayland connection state.
///
/// This struct manages a buffered Wayland socket, keeps track of objects and request/event queues
/// and dispatches object events.
///
/// Set `WAYLAND_DEBUG=1` environment variable to get debug messages.
pub struct Connection<D> {
    #[cfg(feature = "tokio")]
    async_fd: Option<AsyncFd<RawFd>>,

    socket: BufferedSocket<UnixStream>,
    msg_buffers_pool: MessageBuffersPool,

    object_mgr: ObjectManager<D>,

    event_queue: VecDeque<QueuedEvent>,
    requests_queue: VecDeque<Message>,
    break_dispatch: bool,

    registry: WlRegistry,
    globals: Vec<GlobalArgs>,

    // This is `None` while dispatching registry events, to prevent mutation from registry callbacks.
    registry_cbs: Option<Vec<RegistryCb<D>>>,

    debug: bool,
}

enum QueuedEvent {
    DeleteId(ObjectId),
    RegistryEvent(wl_registry::Event),
    Message(Message),
}

pub(crate) type GenericCallback<D> =
    Box<dyn FnMut(&mut Connection<D>, &mut D, Object, Message) + Send>;

type RegistryCb<D> = Box<dyn FnMut(&mut Connection<D>, &mut D, &wl_registry::Event) + Send>;

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
        let runtime_dir = env::var_os("XDG_RUNTIME_DIR").ok_or(ConnectError::NotEnoughEnvVars)?;
        let wayland_disp = env::var_os("WAYLAND_DISPLAY").ok_or(ConnectError::NotEnoughEnvVars)?;

        let mut path = PathBuf::new();
        path.push(runtime_dir);
        path.push(wayland_disp);

        let mut this = Self {
            #[cfg(feature = "tokio")]
            async_fd: None,

            socket: BufferedSocket::from(UnixStream::connect(path)?),
            msg_buffers_pool: MessageBuffersPool::default(),

            object_mgr: ObjectManager::new(),

            event_queue: VecDeque::with_capacity(32),
            requests_queue: VecDeque::with_capacity(32),
            break_dispatch: false,

            registry: WlRegistry::new(ObjectId::MAX_CLIENT, 1), // Temp dummy object
            globals: Vec::new(),
            registry_cbs: Some(Vec::new()),

            debug: std::env::var_os("WAYLAND_DEBUG").is_some(),
        };

        this.registry = WlDisplay::INSTANCE.get_registry(&mut this);

        Ok(this)
    }

    /// [`connect`](Self::connect) and collect the initial set of advertised globals.
    ///
    /// This will empty the event queue, so no callbacks will be called on the received globals.
    #[deprecated = "use blocking_roundtrip() + bind_singleton() instead"]
    pub fn connect_and_collect_globals() -> Result<(Self, Vec<GlobalArgs>), ConnectError> {
        let mut this = Self::connect()?;
        this.blocking_roundtrip()?;
        let globals = this.globals.clone();
        this.event_queue.clear();
        Ok((this, globals))
    }

    /// Async version of [`connect_and_collect_globals`](Self::connect_and_collect_globals).
    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
    #[deprecated = "use async_roundtrip() + bind_singleton() instead"]
    pub async fn async_connect_and_collect_globals() -> Result<(Self, Vec<GlobalArgs>), ConnectError>
    {
        let mut this = Self::connect()?;
        this.async_roundtrip().await?;
        let globals = this.globals.clone();
        this.event_queue.clear();
        Ok((this, globals))
    }

    /// Get Wayland registry.
    ///
    /// At the moment, only a single registry can be created. This might or might not change in the
    /// future, considering registries cannot be destroyed.
    pub fn registry(&self) -> WlRegistry {
        self.registry
    }

    /// Get a list of available globals.
    ///
    /// The order of globals is not specified.
    ///
    /// Note that this function has knowledge of all events received from the compositor, even the
    /// ones that had not been dispatched in [`dispatch_events`](Self::dispatch_events) yet.
    pub fn globals(&self) -> &[GlobalArgs] {
        &self.globals
    }

    /// Bind a singleton global.
    ///
    /// Use this function to only bind singleton globals. If more than one global of the requeseted
    /// interface is available, the behaviour is not specified.
    ///
    /// Note that this function has knowledge of all events received from the compositor, even the
    /// ones that had not been dispatched in [`dispatch_events`](Self::dispatch_events) yet.
    ///
    /// The version argmuent can be a:
    /// - Number - require a specific version
    /// - Range to inclusive (`..=b` - bind a version in range `[1, b]`)
    /// - Range inclusive (`a..=b` - bind a version in range `[a, b]`)
    pub fn bind_singleton<P: Proxy>(
        &mut self,
        version: impl VersionBounds,
    ) -> Result<P, BindError> {
        assert!(version.upper() <= P::INTERFACE.version);

        let i = self
            .globals
            .iter()
            .position(|g| g.is::<P>())
            .ok_or(BindError::GlobalNotFound(P::INTERFACE.name))?;

        if self.globals[i].version < version.lower() {
            return Err(BindError::UnsupportedVersion {
                actual: self.globals[i].version,
                min: version.lower(),
            });
        }

        let name = self.globals[i].name;
        let version = u32::min(version.upper(), self.globals[i].version);
        Ok(self.registry.bind(self, name, version))
    }

    /// Same as [`bind_singleton`](Self::bind_singleton) but also sets the callback
    pub fn bind_singleton_with_cb<P: Proxy, F: FnMut(EventCtx<D, P>) + Send + 'static>(
        &mut self,
        version: impl VersionBounds,
        cb: F,
    ) -> Result<P, BindError> {
        assert!(version.upper() <= P::INTERFACE.version);

        let i = self
            .globals
            .iter()
            .position(|g| g.is::<P>())
            .ok_or(BindError::GlobalNotFound(P::INTERFACE.name))?;

        if self.globals[i].version < version.lower() {
            return Err(BindError::UnsupportedVersion {
                actual: self.globals[i].version,
                min: version.lower(),
            });
        }

        let name = self.globals[i].name;
        let version = u32::min(version.upper(), self.globals[i].version);
        Ok(self.registry.bind_with_cb(self, name, version, cb))
    }

    /// Register a registry event callback.
    ///
    /// In this library, `wl_registry` is the only object which can have any number of callbacks,
    /// which are triggered in the order in which they were added.
    ///
    /// # Panics
    ///
    /// This method panics if called from the context of a registry callback.
    pub fn add_registry_cb<
        F: FnMut(&mut Connection<D>, &mut D, &wl_registry::Event) + Send + 'static,
    >(
        &mut self,
        cb: F,
    ) {
        self.registry_cbs
            .as_mut()
            .expect("add_registry_cb called from registry callback")
            .push(Box::new(cb));
    }

    /// Set a callback for a given object.
    ///
    /// # Panics
    ///
    /// This method panics if current set of objects does not contain an object with id identical
    /// to `proxy.id()`, internally stored object differs from `proxy` or object is dead.
    ///
    /// It also panics if `proxy` is a `wl_registry`. Use [`add_registry_cb`](Self::add_registry_cb) to listen to
    /// registry events.
    ///
    /// Calling this function on a destroyed object will most likely panic, but this is not
    /// guarantied due to id-reuse.
    pub fn set_callback_for<P: Proxy, F: FnMut(EventCtx<D, P>) + Send + 'static>(
        &mut self,
        proxy: P,
        cb: F,
    ) {
        assert_ne!(
            P::INTERFACE,
            WlRegistry::INTERFACE,
            "attempt to set a callback for wl_registry"
        );

        let obj = self
            .object_mgr
            .get_object_mut(proxy.id())
            .expect("attempt to set a callback for non-existing object");

        assert_eq!(obj.object, proxy.id(), "object mismatch");
        assert!(obj.is_alive, "attempt to set a callback for dead object");

        obj.cb = Some(Self::make_generic_cb(cb));
    }

    /// Remove all callbacks.
    ///
    /// You can use this function to change the "state type" of a connection.
    pub fn clear_callbacks<D2>(self) -> Connection<D2> {
        Connection {
            #[cfg(feature = "tokio")]
            async_fd: self.async_fd,
            socket: self.socket,
            msg_buffers_pool: self.msg_buffers_pool,
            object_mgr: self.object_mgr.clear_callbacks(),
            event_queue: self.event_queue,
            requests_queue: self.requests_queue,
            break_dispatch: self.break_dispatch,
            registry: self.registry,
            globals: self.globals,
            registry_cbs: Some(Vec::new()),
            debug: self.debug,
        }
    }

    /// Perform a blocking roundtrip.
    ///
    /// This function flushes the buffer of pending requests. All received events during the
    /// roundtrip are queued.
    pub fn blocking_roundtrip(&mut self) -> io::Result<()> {
        let sync_cb = WlDisplay::INSTANCE.sync(self);
        self.flush(IoMode::Blocking)?;

        loop {
            match self.recv_event(IoMode::Blocking)? {
                QueuedEvent::Message(m) if m.header.object_id == sync_cb => break,
                other => self.event_queue.push_back(other),
            }
        }

        Ok(())
    }

    /// Async version of [`blocking_roundtrip`](Self::blocking_roundtrip).
    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
    pub async fn async_roundtrip(&mut self) -> io::Result<()> {
        let sync_cb = WlDisplay::INSTANCE.sync(self);
        self.async_flush().await?;

        loop {
            match self.async_recv_event().await? {
                QueuedEvent::Message(m) if m.header.object_id == sync_cb => break,
                other => self.event_queue.push_back(other),
            }
        }

        Ok(())
    }

    #[doc(hidden)]
    pub fn alloc_msg_args(&mut self) -> Vec<ArgValue> {
        self.msg_buffers_pool.get_args()
    }

    #[doc(hidden)]
    pub fn send_request(&mut self, iface: &'static Interface, request: Message) {
        let obj = self
            .object_mgr
            .get_object_mut(request.header.object_id)
            .expect("attempt to send request for non-existing object");
        assert!(obj.is_alive, "attempt to send request for dead object");

        if self.debug {
            eprintln!(
                "[wayrs]  -> {:?}",
                DebugMessage::new(&request, false, obj.object)
            );
        }

        // Destroy object if request is destrctor
        if iface.requests[request.header.opcode as usize].is_destructor {
            obj.is_alive = false;
        }

        // Queue request
        self.requests_queue.push_back(request);
    }

    fn recv_event(&mut self, mode: IoMode) -> io::Result<QueuedEvent> {
        loop {
            let header = self
                .socket
                .peek_message_header(mode)
                .map_err(|err| match err {
                    PeekHeaderError::Io(io) => io,
                    other => io::Error::new(io::ErrorKind::InvalidData, other),
                })?;

            let obj = self
                .object_mgr
                .get_object_mut(header.object_id)
                .expect("received event for non-existing object");
            let object = obj.object;
            let signature = object
                .interface
                .events
                .get(header.opcode as usize)
                .expect("incorrect opcode")
                .signature;

            let event = self
                .socket
                .recv_message(header, signature, &mut self.msg_buffers_pool, mode)
                .map_err(|err| match err {
                    RecvMessageError::Io(io) => io,
                    other => io::Error::new(io::ErrorKind::InvalidData, other),
                })?;
            if self.debug {
                eprintln!("[wayrs] {:?}", DebugMessage::new(&event, true, object));
            }

            if event.header.object_id == ObjectId::DISPLAY {
                match WlDisplay::parse_event(event, 1, &mut self.msg_buffers_pool).unwrap() {
                    wl_display::Event::Error(err) => {
                        // Catch protocol error as early as possible
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            format!(
                                "Error in object {} (code({})): {}",
                                err.object_id.0,
                                err.code,
                                err.message.to_string_lossy(),
                            ),
                        ));
                    }
                    wl_display::Event::DeleteId(id) => {
                        return Ok(QueuedEvent::DeleteId(ObjectId(
                            NonZeroU32::new(id).ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "wl_display.delete_id with null id",
                                )
                            })?,
                        )));
                    }
                };
            }

            if event.header.object_id == self.registry {
                let event = WlRegistry::parse_event(event, 1, &mut self.msg_buffers_pool).unwrap();
                match &event {
                    wl_registry::Event::Global(global) => {
                        self.globals.push(global.clone());
                    }
                    wl_registry::Event::GlobalRemove(name) => {
                        if let Some(i) = self.globals.iter().position(|g| g.name == *name) {
                            self.globals.swap_remove(i);
                        }
                    }
                }
                return Ok(QueuedEvent::RegistryEvent(event));
            }

            // Allocate objects if necessary
            let signature = object
                .interface
                .events
                .get(header.opcode as usize)
                .expect("incorrect opcode")
                .signature;
            for (arg, arg_ty) in event.args.iter().zip(signature) {
                match arg {
                    ArgValue::NewId(id) => {
                        let ArgType::NewId(interface) = arg_ty else {
                            unreachable!()
                        };
                        self.object_mgr.register_server_object(Object {
                            id: *id,
                            interface,
                            version: object.version,
                        });
                    }
                    ArgValue::AnyNewId(_, _, _) => unimplemented!(),
                    _ => (),
                }
            }

            return Ok(QueuedEvent::Message(event));
        }
    }

    #[cfg(feature = "tokio")]
    async fn async_recv_event(&mut self) -> io::Result<QueuedEvent> {
        let mut async_fd = match self.async_fd.take() {
            Some(fd) => fd,
            None => AsyncFd::new(self.as_raw_fd())?,
        };

        loop {
            let mut fd_guard = async_fd.readable_mut().await?;
            match self.recv_event(IoMode::NonBlocking) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => fd_guard.clear_ready(),
                result => {
                    self.async_fd = Some(async_fd);
                    return result;
                }
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
        let msg = self.async_recv_event().await?;
        self.event_queue.push_back(msg);

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
            if let Err(SendMessageError { msg, err }) =
                self.socket
                    .write_message(msg, &mut self.msg_buffers_pool, mode)
            {
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
        // Try to just flush before even touching async fd. In many cases flushing does not block.
        match self.flush(IoMode::NonBlocking) {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
            result => return result,
        }

        let mut async_fd = match self.async_fd.take() {
            Some(fd) => fd,
            None => AsyncFd::new(self.as_raw_fd())?,
        };

        loop {
            let mut fd_guard = async_fd.writable_mut().await?;
            match self.flush(IoMode::NonBlocking) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => fd_guard.clear_ready(),
                result => {
                    self.async_fd = Some(async_fd);
                    return result;
                }
            }
        }
    }

    /// Empty the queue of pending events, calling a callback (if set) for each event.
    ///
    /// # Panics
    ///
    /// This method panics if called from the context of a callback.
    pub fn dispatch_events(&mut self, state: &mut D) {
        self.break_dispatch = false;

        while let Some(event) = self.event_queue.pop_front() {
            match event {
                QueuedEvent::DeleteId(id) => self.object_mgr.delete_client_object(id),
                QueuedEvent::RegistryEvent(event) => {
                    let mut registry_cbs = self
                        .registry_cbs
                        .take()
                        .expect("dispatch_events called from registry callback");

                    for cb in &mut registry_cbs {
                        cb(self, state, &event);
                    }

                    self.registry_cbs = Some(registry_cbs);

                    if self.break_dispatch {
                        break;
                    }
                }
                QueuedEvent::Message(event) => {
                    let object = match self.object_mgr.get_object_mut(event.header.object_id) {
                        Some(obj) if obj.is_alive => obj,
                        _ => continue, // Ignore unknown/dead objects
                    };

                    // Removing the callback from the object to make borrow checker happy
                    let mut object_cb = object.cb.take();
                    let object = object.object;
                    let opcode = event.header.opcode;

                    if let Some(cb) = &mut object_cb {
                        cb(self, state, object, event);
                    }

                    let object = self.object_mgr.get_object_mut(object.id).unwrap();

                    // Destroy object if event is destructor.
                    if object.object.interface.events[opcode as usize].is_destructor {
                        object.is_alive = false;
                    }

                    // Re-add callback if it wasn't re-set in the callback
                    if object.is_alive && object.cb.is_none() {
                        object.cb = object_cb;
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
    #[doc(hidden)]
    pub fn allocate_new_object<P: Proxy>(&mut self, version: u32) -> P {
        let id = self
            .object_mgr
            .alloc_client_object(P::INTERFACE, version)
            .object
            .id;
        P::new(id, version)
    }

    /// Allocate a new object and set callback. Returned object must be sent in a request as a
    /// "new_id" argument.
    #[doc(hidden)]
    pub fn allocate_new_object_with_cb<P: Proxy, F: FnMut(EventCtx<D, P>) + Send + 'static>(
        &mut self,
        version: u32,
        cb: F,
    ) -> P {
        let state = self.object_mgr.alloc_client_object(P::INTERFACE, version);
        state.cb = Some(Self::make_generic_cb(cb));
        P::new(state.object.id, version)
    }

    fn make_generic_cb<P: Proxy, F: FnMut(EventCtx<D, P>) + Send + 'static>(
        mut cb: F,
    ) -> GenericCallback<D> {
        // Note: if `F` does not capture anything, this `Box::new` will not allocate.
        Box::new(move |conn, state, object, event| {
            let proxy: P = object.try_into().unwrap();
            let event = P::parse_event(event, object.version, &mut conn.msg_buffers_pool).unwrap();
            let ctx = EventCtx {
                conn,
                state,
                proxy,
                event,
            };
            cb(ctx);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn send() {
        assert_send::<Connection<()>>();
    }
}
