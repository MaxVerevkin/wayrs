//! An example of how to use your own custom transport implementation.
//! Here, the transport keeps track of how much bytes were sent/received.

use std::collections::VecDeque;
use std::env;
use std::io;
use std::os::fd::{OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use wayrs_client::core::transport::Transport;
use wayrs_client::protocol::wl_registry;
use wayrs_client::{Connection, IoMode};

fn main() {
    let mut conn = Connection::with_transport(MyTransport::connect());

    conn.add_registry_cb(|_conn, _state, event| match event {
        wl_registry::Event::Global(g) => println!(
            "global ({}) {} added",
            g.name,
            g.interface.to_string_lossy(),
        ),
        wl_registry::Event::GlobalRemove(name) => println!("global ({name}) removed"),
    });

    loop {
        conn.flush(IoMode::Blocking).unwrap();
        conn.recv_events(IoMode::Blocking).unwrap();
        conn.dispatch_events(&mut ());

        let t = conn.transport::<MyTransport>().unwrap();
        eprintln!("up: {}b down: {}b", t.bytes_sent, t.bytes_read);
    }
}

struct MyTransport {
    socket: UnixStream,
    bytes_read: usize,
    bytes_sent: usize,
}

impl Transport for MyTransport {
    fn pollable_fd(&self) -> RawFd {
        self.socket.pollable_fd()
    }

    fn send(
        &mut self,
        bytes: &[std::io::IoSlice],
        fds: &[OwnedFd],
        mode: IoMode,
    ) -> io::Result<usize> {
        let n = self.socket.send(bytes, fds, mode)?;
        self.bytes_sent += n;
        Ok(n)
    }

    fn recv(
        &mut self,
        bytes: &mut [std::io::IoSliceMut],
        fds: &mut VecDeque<OwnedFd>,
        mode: IoMode,
    ) -> io::Result<usize> {
        let n = self.socket.recv(bytes, fds, mode)?;
        self.bytes_read += n;
        Ok(n)
    }
}

impl MyTransport {
    fn connect() -> Self {
        let runtime_dir = env::var_os("XDG_RUNTIME_DIR").unwrap();
        let wayland_disp = env::var_os("WAYLAND_DISPLAY").unwrap();

        let mut path = PathBuf::new();
        path.push(runtime_dir);
        path.push(wayland_disp);

        Self {
            socket: UnixStream::connect(path).unwrap(),
            bytes_read: 0,
            bytes_sent: 0,
        }
    }
}
