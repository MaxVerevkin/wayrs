use std::collections::{HashMap, VecDeque};
use std::io::{self, ErrorKind};

use crate::connection::{Connection, WlDisplay};
use crate::interface::Interface;
use crate::object::ObjectId;
use crate::protocol::wl_registry::{self, GlobalArgs, WlRegistry};
use crate::proxy::{make_callback, Dispatch, Dispatcher, EventCallback, Proxy};
use crate::socket::IoMode;
use crate::wire::Message;
use crate::ConnectError;

pub struct EventQueue<D: Dispatcher> {
    conn: Connection,
    registry: WlRegistry,
    event_queue: VecDeque<Message>,
    callbacks: HashMap<&'static Interface, EventCallback<D>>,
    break_dispatch: bool,
}

impl<D: Dispatcher> EventQueue<D> {
    pub fn blocking_init() -> Result<(Vec<GlobalArgs>, Self), ConnectError>
    where
        D: Dispatch<WlRegistry>,
    {
        let mut connection = Connection::connect()?;
        let registry = WlDisplay.get_registry(&mut connection);

        let mut event_queue = Self {
            conn: connection,
            registry,
            event_queue: VecDeque::new(),
            callbacks: HashMap::new(),
            break_dispatch: false,
        };

        event_queue.blocking_roundtrip()?;

        let mut globals = Vec::new();
        for event in event_queue.event_queue.drain(..) {
            assert_eq!(event.header.object_id, registry.id());
            match event.try_into().unwrap() {
                wl_registry::Event::Global(global) => globals.push(global),
                wl_registry::Event::GlobalRemove(name) => {
                    globals.retain(|g| g.name != name);
                }
            }
        }

        event_queue.set_callback::<WlRegistry>();

        Ok((globals, event_queue))
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

    pub fn connection(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn recv_events(&mut self, mut mode: IoMode) -> io::Result<()> {
        let mut at_least_one = false;

        loop {
            let msg = match self.conn.recv_event(mode) {
                Ok(msg) => msg,
                Err(e) if e.kind() == ErrorKind::WouldBlock && at_least_one => return Ok(()),
                Err(e) => return Err(e),
            };

            at_least_one = true;
            mode = IoMode::NonBlocking;
            self.event_queue.push_back(msg);
        }
    }

    pub fn dispatch_events(&mut self, state: &mut D) -> Result<(), D::Error> {
        while let Some(event) = self.event_queue.pop_front() {
            if event.header.object_id == ObjectId::DISPLAY {
                self.conn.process_dispay_event(event);
                continue;
            }

            let Some(object) = self
                .conn
                .get_object(event.header.object_id)
            else { continue };

            // for x in self.callbacks.keys() {
            //     let x: &'static Interface = x;
            //     eprintln!("{x:p}: {:?}", x.name);
            // }
            //
            // let x: &'static Interface = object.interface;
            // eprintln!("-> {x:p}: {:?}", x.name);

            let cb = self
                .callbacks
                .get(&object.interface)
                .expect("dispatch callback not found");

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

    pub fn blocking_roundtrip(&mut self) -> io::Result<()> {
        let sync_cb = WlDisplay.sync(&mut self.conn);
        self.conn.flush(IoMode::Blocking)?;

        loop {
            let event = self.conn.recv_event(IoMode::Blocking)?;
            match event.header.object_id {
                id if id == sync_cb.id() => return Ok(()),
                _ => self.event_queue.push_back(event),
            }
        }
    }
}
