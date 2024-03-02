use std::collections::VecDeque;
use std::io::{self, IoSlice, IoSliceMut};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

use nix::sys::socket::{self, ControlMessage, ControlMessageOwned};

use super::{Transport, FDS_OUT_LEN};
use crate::IoMode;

/// Wayland transport over unix domain socket
///
/// This is the most commonly used Wayland transport method.
pub struct Unix {
    socket: UnixStream,
    cmsg: Vec<u8>,
}

impl AsRawFd for Unix {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

impl From<UnixStream> for Unix {
    fn from(socket: UnixStream) -> Self {
        Self {
            socket,
            cmsg: nix::cmsg_space!([RawFd; FDS_OUT_LEN]),
        }
    }
}

impl Transport for Unix {
    fn send(
        &mut self,
        bytes: &[IoSlice],
        fds: &VecDeque<OwnedFd>,
        mode: IoMode,
    ) -> io::Result<usize> {
        let mut flags = socket::MsgFlags::MSG_NOSIGNAL;
        if mode == IoMode::NonBlocking {
            flags |= socket::MsgFlags::MSG_DONTWAIT;
        }

        let b;
        let mut fds_array = [0; FDS_OUT_LEN];
        for (i, fd) in fds.iter().enumerate() {
            fds_array[i] = fd.as_raw_fd();
        }
        let cmsgs: &[ControlMessage] = if fds.is_empty() {
            &[]
        } else {
            b = [ControlMessage::ScmRights(&fds_array[..fds.len()])];
            &b
        };

        let sent = socket::sendmsg::<()>(self.socket.as_raw_fd(), bytes, cmsgs, flags, None)?;
        Ok(sent)
    }

    fn recv(
        &mut self,
        bytes: &mut [IoSliceMut],
        fds: &mut VecDeque<OwnedFd>,
        mode: IoMode,
    ) -> io::Result<usize> {
        self.cmsg.clear();

        let mut flags = socket::MsgFlags::MSG_CMSG_CLOEXEC | socket::MsgFlags::MSG_NOSIGNAL;
        if mode == IoMode::NonBlocking {
            flags |= socket::MsgFlags::MSG_DONTWAIT;
        }

        let msg =
            socket::recvmsg::<()>(self.socket.as_raw_fd(), bytes, Some(&mut self.cmsg), flags)?;

        for cmsg in msg.cmsgs() {
            if let ControlMessageOwned::ScmRights(fds_vec) = cmsg {
                for fd in fds_vec {
                    assert_ne!(fd, -1);
                    fds.push_back(unsafe { OwnedFd::from_raw_fd(fd) });
                }
            }
        }

        let read = msg.bytes;

        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "server disconnected",
            ));
        }

        Ok(read)
    }
}
