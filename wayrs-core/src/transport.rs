//! Wayland transport methods

use std::borrow::Cow;
use std::collections::VecDeque;
use std::ffi::CString;
use std::io::{self, IoSlice, IoSliceMut};
use std::num::NonZeroU32;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};

use crate::ring_buffer::RingBuffer;
use crate::{
    ArgType, ArgValue, Fixed, IoMode, Message, MessageBuffersPool, MessageHeader, ObjectId,
};

mod unix;

pub const BYTES_OUT_LEN: usize = 4096;
pub const BYTES_IN_LEN: usize = BYTES_OUT_LEN * 2;
pub const FDS_OUT_LEN: usize = 28;
pub const FDS_IN_LEN: usize = FDS_OUT_LEN * 2;

/// A buffered Wayland socket
///
/// Handles message marshalling and unmarshalling. This struct is generic over [`Transport`], which
/// is usually [`UnixStream`](std::os::unix::net::UnixStream).
///
/// To create a new instance, use the `From<T: Transport>` implementation.
pub struct BufferedSocket<T> {
    socket: T,
    bytes_in: RingBuffer,
    bytes_out: RingBuffer,
    fds_in: VecDeque<OwnedFd>,
    fds_out: VecDeque<OwnedFd>,
}

/// An abstraction over Wayland transport methods
pub trait Transport {
    fn pollable_fd(&self) -> RawFd;

    fn send(&mut self, bytes: &[IoSlice], fds: &[OwnedFd], mode: IoMode) -> io::Result<usize>;

    fn recv(
        &mut self,
        bytes: &mut [IoSliceMut],
        fds: &mut VecDeque<OwnedFd>,
        mode: IoMode,
    ) -> io::Result<usize>;
}

impl<T: Transport> AsRawFd for BufferedSocket<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.pollable_fd()
    }
}

impl<T: Transport> From<T> for BufferedSocket<T> {
    fn from(socket: T) -> Self {
        Self {
            socket,
            bytes_in: RingBuffer::new(BYTES_IN_LEN),
            bytes_out: RingBuffer::new(BYTES_OUT_LEN),
            fds_in: VecDeque::new(),
            fds_out: VecDeque::new(),
        }
    }
}

/// An error occurred while sending a message
pub struct SendMessageError {
    pub msg: Message,
    pub err: io::Error,
}

/// An error occured while trying to receive a message
#[derive(Debug, thiserror::Error)]
pub enum RecvMessageError {
    #[error("io: {0}")]
    Io(io::Error),
    #[error("message has too many file descriptors")]
    TooManyFds,
    #[error("message is too large")]
    TooManyBytes,
    #[error("message contains unexpected null")]
    UnexpectedNull,
    #[error("message contains null byte in a string")]
    NullInString,
}

/// An error occured while trying to receive a message
#[derive(Debug, thiserror::Error)]
pub enum PeekHeaderError {
    #[error("io: {0}")]
    Io(io::Error),
    #[error("header has a null object id")]
    NullObject,
}

impl<T: Transport> BufferedSocket<T> {
    /// Write a single Wayland message into the intevnal buffer.
    ///
    /// Flushes the buffer if neccessary. On failure, ownership of the message is returned.
    ///
    /// # Panics
    ///
    /// This function panics if the message size is larger than `BYTES_OUT_LEN` or it contains more
    /// than `FDS_OUT_LEN` file descriptors.
    pub fn write_message(
        &mut self,
        msg: Message,
        msg_pool: &mut MessageBuffersPool,
        mode: IoMode,
    ) -> Result<(), SendMessageError> {
        // Calc size
        let size = MessageHeader::SIZE + msg.args.iter().map(ArgValue::size).sum::<usize>();
        let fds_cnt = msg
            .args
            .iter()
            .filter(|arg| matches!(arg, ArgValue::Fd(_)))
            .count();

        // Check size and flush if neccessary
        assert!(size <= BYTES_OUT_LEN);
        assert!(fds_cnt <= FDS_OUT_LEN);
        if size > self.bytes_out.writable_len() || fds_cnt + self.fds_out.len() > FDS_OUT_LEN {
            if let Err(err) = self.flush(mode) {
                return Err(SendMessageError { msg, err });
            }
        }

        // Header
        self.bytes_out.write_uint(msg.header.object_id.0.get());
        self.bytes_out
            .write_uint((size as u32) << 16 | msg.header.opcode as u32);

        // Args
        let mut msg = msg;
        for arg in msg.args.drain(..) {
            match arg {
                ArgValue::Uint(x) => self.bytes_out.write_uint(x),
                ArgValue::Int(x) | ArgValue::Fixed(Fixed(x)) => self.bytes_out.write_int(x),
                ArgValue::Object(ObjectId(x))
                | ArgValue::OptObject(Some(ObjectId(x)))
                | ArgValue::NewId(ObjectId(x)) => self.bytes_out.write_uint(x.get()),
                ArgValue::OptObject(None) | ArgValue::OptString(None) => {
                    self.bytes_out.write_uint(0)
                }
                ArgValue::AnyNewId(iface, version, id) => {
                    self.send_array(iface.to_bytes_with_nul());
                    self.bytes_out.write_uint(version);
                    self.bytes_out.write_uint(id.0.get());
                }
                ArgValue::String(string) | ArgValue::OptString(Some(string)) => {
                    self.send_array(string.to_bytes_with_nul())
                }
                ArgValue::Array(array) => self.send_array(&array),
                ArgValue::Fd(fd) => self.fds_out.push_back(fd),
            }
        }
        msg_pool.reuse_args(msg.args);
        Ok(())
    }

    /// Peek the next message header.
    ///
    /// Fills the internal buffer if needed and keeps the header in the buffer.
    pub fn peek_message_header(&mut self, mode: IoMode) -> Result<MessageHeader, PeekHeaderError> {
        while self.bytes_in.readable_len() < MessageHeader::SIZE {
            self.fill_incoming_buf(mode).map_err(PeekHeaderError::Io)?;
        }

        let mut raw = [0; MessageHeader::SIZE];
        self.bytes_in.peek_bytes(&mut raw);
        let object_id = u32::from_ne_bytes(raw[0..4].try_into().unwrap());
        let size_and_opcode = u32::from_ne_bytes(raw[4..8].try_into().unwrap());

        Ok(MessageHeader {
            object_id: ObjectId(NonZeroU32::new(object_id).ok_or(PeekHeaderError::NullObject)?),
            size: ((size_and_opcode & 0xFFFF_0000) >> 16) as u16,
            opcode: (size_and_opcode & 0x0000_FFFF) as u16,
        })
    }

    /// Receive the entire next message.
    ///
    /// Fills the internal buffer if needed. `header` must be the value returned by
    /// [`Self::peek_message_header`] right before calling this function.
    pub fn recv_message(
        &mut self,
        header: MessageHeader,
        signature: &[ArgType],
        msg_pool: &mut MessageBuffersPool,
        mode: IoMode,
    ) -> Result<Message, RecvMessageError> {
        // Check size and fill buffer if necessary
        let fds_cnt = signature
            .iter()
            .filter(|arg| matches!(arg, ArgType::Fd))
            .count();
        if header.size as usize > BYTES_IN_LEN {
            return Err(RecvMessageError::TooManyBytes);
        }
        if fds_cnt > FDS_IN_LEN {
            return Err(RecvMessageError::TooManyFds);
        }
        while header.size as usize > self.bytes_in.readable_len() || fds_cnt > self.fds_in.len() {
            self.fill_incoming_buf(mode).map_err(RecvMessageError::Io)?;
        }

        // Consume header
        self.bytes_in.move_tail(MessageHeader::SIZE);

        let mut args = msg_pool.get_args();
        for arg_type in signature {
            args.push(match arg_type {
                ArgType::Int => ArgValue::Int(self.bytes_in.read_int()),
                ArgType::Uint => ArgValue::Uint(self.bytes_in.read_uint()),
                ArgType::Fixed => ArgValue::Fixed(Fixed(self.bytes_in.read_int())),
                ArgType::Object => ArgValue::Object(
                    self.bytes_in
                        .read_id()
                        .ok_or(RecvMessageError::UnexpectedNull)?,
                ),
                ArgType::OptObject => ArgValue::OptObject(self.bytes_in.read_id()),
                ArgType::NewId(_interface) => ArgValue::NewId(
                    self.bytes_in
                        .read_id()
                        .ok_or(RecvMessageError::UnexpectedNull)?,
                ),
                ArgType::AnyNewId => ArgValue::AnyNewId(
                    Cow::Owned(self.recv_string()?),
                    self.bytes_in.read_uint(),
                    self.bytes_in
                        .read_id()
                        .ok_or(RecvMessageError::UnexpectedNull)?,
                ),
                ArgType::String => ArgValue::String(self.recv_string()?),
                ArgType::OptString => ArgValue::OptString(match self.bytes_in.read_uint() {
                    0 => None,
                    len => Some(self.recv_string_with_len(len)?),
                }),
                ArgType::Array => ArgValue::Array(self.recv_array()),
                ArgType::Fd => ArgValue::Fd(self.fds_in.pop_front().unwrap()),
            });
        }

        Ok(Message { header, args })
    }

    /// Flush all pending messages.
    pub fn flush(&mut self, mode: IoMode) -> io::Result<()> {
        while !self.bytes_out.is_empty() {
            let mut iov_buf = [IoSlice::new(&[]), IoSlice::new(&[])];
            let iov = self.bytes_out.get_readable_iov(&mut iov_buf);

            let sent = self
                .socket
                .send(iov, self.fds_out.make_contiguous(), mode)?;

            self.bytes_out.move_tail(sent);
            self.fds_out.clear();
        }

        Ok(())
    }

    /// Get a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.socket
    }

    /// Get a mutable reference to the underlying transport.
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.socket
    }

    fn fill_incoming_buf(&mut self, mode: IoMode) -> io::Result<()> {
        if self.bytes_in.is_full() {
            return Ok(());
        }

        let mut iov_buf = [IoSliceMut::new(&mut []), IoSliceMut::new(&mut [])];
        let iov = self.bytes_in.get_writeable_iov(&mut iov_buf);

        let read = self.socket.recv(iov, &mut self.fds_in, mode)?;
        self.bytes_in.move_head(read);

        Ok(())
    }

    fn send_array(&mut self, array: &[u8]) {
        let len = array.len() as u32;

        self.bytes_out.write_uint(len);
        self.bytes_out.write_bytes(array);

        let padding = ((4 - (len % 4)) % 4) as usize;
        self.bytes_out.write_bytes(&[0, 0, 0][..padding]);
    }

    fn recv_array(&mut self) -> Vec<u8> {
        let len = self.bytes_in.read_uint() as usize;

        let mut buf = vec![0; len];
        self.bytes_in.read_bytes(&mut buf);

        let padding = (4 - (len % 4)) % 4;
        self.bytes_in.move_tail(padding);

        buf
    }

    fn recv_string_with_len(&mut self, len: u32) -> Result<CString, RecvMessageError> {
        let mut buf = vec![0; len as usize];
        self.bytes_in.read_bytes(&mut buf);

        let padding = (4 - (len % 4)) % 4;
        self.bytes_in.move_tail(padding as usize);

        CString::from_vec_with_nul(buf).map_err(|_| RecvMessageError::NullInString)
    }

    fn recv_string(&mut self) -> Result<CString, RecvMessageError> {
        let len = self.bytes_in.read_uint();
        if len == 0 {
            Err(RecvMessageError::UnexpectedNull)
        } else {
            self.recv_string_with_len(len)
        }
    }
}
