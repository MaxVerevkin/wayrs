//! Core Wayland functionality
//!
//! It can be used on both client and server side.

use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::{self, IoSlice, IoSliceMut};
use std::num::NonZeroU32;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

use nix::sys::socket::{self, ControlMessage, ControlMessageOwned};

/// The "mode" of an IO operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoMode {
    /// Blocking.
    ///
    /// The function call may block, but it will never return [WouldBlock](io::ErrorKind::WouldBlock)
    /// error.
    Blocking,
    /// Non-blocking.
    ///
    /// The function call will not block on IO operations. [WouldBlock](io::ErrorKind::WouldBlock)
    /// error is returned if the operation cannot be completed immediately.
    NonBlocking,
}

/// A Wayland object ID.
///
/// Uniquely identifies an object at each point of time. Note that an ID may have a limited
/// lifetime. Also an ID which once pointed to a certain object, may point to a different object in
/// the future, due to ID reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub NonZeroU32);

impl ObjectId {
    pub const DISPLAY: Self = Self(unsafe { NonZeroU32::new_unchecked(1) });
    pub const MAX_CLIENT: Self = Self(unsafe { NonZeroU32::new_unchecked(0xFEFFFFFF) });
    pub const MIN_SERVER: Self = Self(unsafe { NonZeroU32::new_unchecked(0xFF000000) });

    /// Returns the numeric representation of the ID
    pub fn as_u32(self) -> u32 {
        self.0.get()
    }

    /// Whether the object with this ID was created by the server
    pub fn created_by_server(self) -> bool {
        self >= Self::MIN_SERVER
    }

    /// Whether the object with this ID was created by the client
    pub fn created_by_client(self) -> bool {
        self <= Self::MAX_CLIENT
    }
}

/// A header of a Wayland message
#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    /// The ID of the associated object
    pub object_id: ObjectId,
    /// Size of the message in bytes, including the header
    pub size: u16,
    /// The opcode of the message
    pub opcode: u16,
}

impl MessageHeader {
    /// The size of the header in bytes
    pub const SIZE: usize = 8;
}

/// A Wayland message
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
    NewId(ObjectId),
    AnyNewId(CString, u32, ObjectId),
    String(CString),
    OptString(Option<CString>),
    Array(Vec<u8>),
    Fd(OwnedFd),
}

impl ArgValue {
    /// The size of the argument in bytes.
    pub fn size(&self) -> usize {
        fn len_with_padding(len: usize) -> usize {
            let padding = (4 - (len % 4)) % 4;
            4 + len + padding
        }

        match self {
            Self::Int(_)
            | Self::Uint(_)
            | Self::Fixed(_)
            | Self::Object(_)
            | Self::OptObject(_)
            | Self::NewId(_)
            | Self::OptString(None) => 4,
            Self::AnyNewId(iface, _version, _id) => {
                len_with_padding(iface.to_bytes_with_nul().len()) + 8
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

impl From<f32> for Fixed {
    fn from(value: f32) -> Self {
        Self((value * 256.0) as i32)
    }
}

impl From<f64> for Fixed {
    fn from(value: f64) -> Self {
        Self((value * 256.0) as i32)
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

impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_f64().fmt(f)
    }
}

/// A Wayland interface, usually generated from the XML files
pub struct Interface {
    pub name: &'static CStr,
    pub version: u32,
    pub events: &'static [MessageDesc],
    pub requests: &'static [MessageDesc],
}

/// A "description" of a single Wayland event or request
#[derive(Debug, Clone, Copy)]
pub struct MessageDesc {
    pub name: &'static str,
    pub is_destructor: bool,
    pub signature: &'static [ArgType],
}

impl PartialEq for &'static Interface {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for &'static Interface {}

impl Hash for &'static Interface {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl fmt::Debug for Interface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Interface").field(&self.name).finish()
    }
}

/// A pool of resources reusable between messages
#[derive(Default)]
pub struct MessageBuffersPool {
    pool: Vec<Vec<ArgValue>>,
}

impl MessageBuffersPool {
    pub fn reuse_args(&mut self, mut buf: Vec<ArgValue>) {
        buf.clear();
        self.pool.push(buf);
    }

    pub fn get_args(&mut self) -> Vec<ArgValue> {
        self.pool.pop().unwrap_or_default()
    }
}

pub const BYTES_OUT_LEN: usize = 4096;
pub const BYTES_IN_LEN: usize = BYTES_OUT_LEN * 2;
pub const FDS_OUT_LEN: usize = 28;
pub const FDS_IN_LEN: usize = FDS_OUT_LEN * 2;

/// A buffered Wayland socket
///
/// Handles message marshalling and unmarshalling.
pub struct BufferedSocket {
    socket: UnixStream,
    bytes_in: buf::RingBuffer<BYTES_IN_LEN>,
    bytes_out: buf::RingBuffer<BYTES_OUT_LEN>,
    fds_in: VecDeque<OwnedFd>,
    fds_out: VecDeque<OwnedFd>,
    cmsg: Vec<u8>,
}

impl AsRawFd for BufferedSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

impl From<UnixStream> for BufferedSocket {
    fn from(socket: UnixStream) -> Self {
        Self {
            socket,
            bytes_in: buf::RingBuffer::new(),
            bytes_out: buf::RingBuffer::new(),
            fds_in: VecDeque::new(),
            fds_out: VecDeque::new(),
            cmsg: nix::cmsg_space!([RawFd; FDS_OUT_LEN]),
        }
    }
}

/// An error occurred while sending a message
pub struct SendMessageError {
    pub msg: Message,
    pub err: io::Error,
}

impl BufferedSocket {
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
        while size > self.bytes_out.writable_len() || fds_cnt + self.fds_out.len() > FDS_OUT_LEN {
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
    pub fn peek_message_header(&mut self, mode: IoMode) -> io::Result<MessageHeader> {
        while self.bytes_in.readable_len() < MessageHeader::SIZE {
            self.fill_incoming_buf(mode)?;
        }

        let mut raw = [0; MessageHeader::SIZE];
        self.bytes_in.peek_bytes(&mut raw);
        let object_id = u32::from_ne_bytes(raw[0..4].try_into().unwrap());
        let size_and_opcode = u32::from_ne_bytes(raw[4..8].try_into().unwrap());

        Ok(MessageHeader {
            object_id: ObjectId(NonZeroU32::new(object_id).expect("received event for null id")),
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
    ) -> io::Result<Message> {
        // Check size and fill buffer if necessary
        let fds_cnt = signature
            .iter()
            .filter(|arg| matches!(arg, ArgType::Fd))
            .count();
        assert!(header.size as usize <= BYTES_IN_LEN);
        assert!(fds_cnt <= FDS_IN_LEN);
        while header.size as usize > self.bytes_in.readable_len() || fds_cnt > self.fds_in.len() {
            self.fill_incoming_buf(mode)?;
        }

        // Consume header
        self.bytes_in.move_tail(MessageHeader::SIZE);

        let mut args = msg_pool.get_args();
        args.extend(signature.iter().map(|arg_type| match arg_type {
            ArgType::Int => ArgValue::Int(self.bytes_in.read_int()),
            ArgType::Uint => ArgValue::Uint(self.bytes_in.read_uint()),
            ArgType::Fixed => ArgValue::Fixed(Fixed(self.bytes_in.read_int())),
            ArgType::Object => {
                ArgValue::Object(self.bytes_in.read_id().expect("unexpected null object id"))
            }
            ArgType::OptObject => ArgValue::OptObject(self.bytes_in.read_id()),
            ArgType::NewId(_interface) => {
                ArgValue::NewId(self.bytes_in.read_id().expect("unexpected null new_id"))
            }
            ArgType::AnyNewId => ArgValue::AnyNewId(
                self.recv_string(),
                self.bytes_in.read_uint(),
                self.bytes_in.read_id().expect("unexpected null new_id"),
            ),
            ArgType::String => ArgValue::String(self.recv_string()),
            ArgType::OptString => ArgValue::OptString(match self.bytes_in.read_uint() {
                0 => None,
                len => Some(self.recv_string_with_len(len)),
            }),
            ArgType::Array => ArgValue::Array(self.recv_array()),
            ArgType::Fd => ArgValue::Fd(self.fds_in.pop_front().unwrap()),
        }));

        Ok(Message { header, args })
    }

    /// Flush all pending messages.
    pub fn flush(&mut self, mode: IoMode) -> io::Result<()> {
        if self.bytes_out.is_empty() && self.fds_out.is_empty() {
            return Ok(());
        }

        let mut flags = socket::MsgFlags::MSG_NOSIGNAL;
        if mode == IoMode::NonBlocking {
            flags |= socket::MsgFlags::MSG_DONTWAIT;
        }

        let b;
        let mut fds = [0; FDS_OUT_LEN];
        for (i, fd) in self.fds_out.iter().enumerate() {
            fds[i] = fd.as_raw_fd();
        }
        let cmsgs: &[ControlMessage] = if fds.is_empty() {
            &[]
        } else {
            b = [ControlMessage::ScmRights(&fds[..self.fds_out.len()])];
            &b
        };

        let mut iov_buf = [IoSlice::new(&[]), IoSlice::new(&[])];
        let iov = self.bytes_out.get_readable_iov(&mut iov_buf);
        let sent = socket::sendmsg::<()>(self.socket.as_raw_fd(), iov, cmsgs, flags, None)?;

        // Does this have to be true?
        assert_eq!(sent, self.bytes_out.readable_len());

        self.bytes_out.clear();
        self.fds_out.clear();

        Ok(())
    }
}

impl BufferedSocket {
    fn fill_incoming_buf(&mut self, mode: IoMode) -> io::Result<()> {
        if self.bytes_in.is_full() {
            return Ok(());
        }

        self.cmsg.clear();

        let mut flags = socket::MsgFlags::MSG_CMSG_CLOEXEC | socket::MsgFlags::MSG_NOSIGNAL;
        if mode == IoMode::NonBlocking {
            flags |= socket::MsgFlags::MSG_DONTWAIT;
        }

        let mut iov_buf = [IoSliceMut::new(&mut []), IoSliceMut::new(&mut [])];
        let iov = self.bytes_in.get_writeable_iov(&mut iov_buf);
        let msg = socket::recvmsg::<()>(self.socket.as_raw_fd(), iov, Some(&mut self.cmsg), flags)?;

        for cmsg in msg.cmsgs() {
            if let ControlMessageOwned::ScmRights(fds) = cmsg {
                for fd in fds {
                    assert_ne!(fd, -1);
                    self.fds_in.push_back(unsafe { OwnedFd::from_raw_fd(fd) });
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

    fn recv_string_with_len(&mut self, len: u32) -> CString {
        let mut buf = vec![0; len as usize];
        self.bytes_in.read_bytes(&mut buf);

        let padding = (4 - (len % 4)) % 4;
        self.bytes_in.move_tail(padding as usize);

        CString::from_vec_with_nul(buf).expect("received string with internal null bytes")
    }

    fn recv_string(&mut self) -> CString {
        let len = self.bytes_in.read_uint();
        self.recv_string_with_len(len)
    }
}

mod buf {
    use super::*;

    pub struct RingBuffer<const N: usize> {
        bytes: Box<[u8; N]>,
        offset: usize,
        len: usize,
    }

    impl<const N: usize> RingBuffer<N> {
        pub fn new() -> Self {
            Self {
                bytes: Box::new([0; N]),
                offset: 0,
                len: 0,
            }
        }

        pub fn clear(&mut self) {
            self.offset = 0;
            self.len = 0;
        }

        pub fn move_head(&mut self, n: usize) {
            self.len += n;
        }

        pub fn move_tail(&mut self, n: usize) {
            self.offset = (self.offset + n) % N;
            self.len = self.len.checked_sub(n).unwrap();
        }

        pub fn readable_len(&self) -> usize {
            self.len
        }

        pub fn writable_len(&self) -> usize {
            N - self.len
        }

        pub fn is_empty(&self) -> bool {
            self.len == 0
        }

        pub fn is_full(&self) -> bool {
            self.len == N
        }

        fn head(&self) -> usize {
            (self.offset + self.len) % N
        }

        pub fn write_bytes(&mut self, data: &[u8]) {
            assert!(self.writable_len() >= data.len());

            let head = self.head();
            if head + data.len() <= N {
                self.bytes[head..][..data.len()].copy_from_slice(data);
            } else {
                let size = N - head;
                let rest = data.len() - size;
                self.bytes[head..][..size].copy_from_slice(&data[..size]);
                self.bytes[..rest].copy_from_slice(&data[size..]);
            }

            self.move_head(data.len());
        }

        pub fn peek_bytes(&mut self, buf: &mut [u8]) {
            assert!(self.readable_len() >= buf.len());

            if self.offset + buf.len() <= N {
                buf.copy_from_slice(&self.bytes[self.offset..][..buf.len()]);
            } else {
                let size = N - self.offset;
                let rest = buf.len() - size;
                buf[..size].copy_from_slice(&self.bytes[self.offset..][..size]);
                buf[size..].copy_from_slice(&self.bytes[..rest]);
            }
        }

        pub fn read_bytes(&mut self, buf: &mut [u8]) {
            self.peek_bytes(buf);
            self.move_tail(buf.len());
        }

        pub fn get_writeable_iov<'b, 'a: 'b>(
            &'a mut self,
            iov_buf: &'b mut [IoSliceMut<'a>; 2],
        ) -> &'b mut [IoSliceMut<'a>] {
            let head = self.head();
            if self.len == 0 {
                self.offset = 0;
                iov_buf[0] = IoSliceMut::new(&mut *self.bytes);
                &mut iov_buf[0..1]
            } else if head < self.offset {
                iov_buf[0] = IoSliceMut::new(&mut self.bytes[head..self.offset]);
                &mut iov_buf[0..1]
            } else if self.offset == 0 {
                iov_buf[0] = IoSliceMut::new(&mut self.bytes[head..N]);
                &mut iov_buf[0..1]
            } else {
                let (left, right) = self.bytes.split_at_mut(head);
                iov_buf[0] = IoSliceMut::new(right);
                iov_buf[1] = IoSliceMut::new(&mut left[..self.offset]);
                &mut iov_buf[0..2]
            }
        }

        pub fn get_readable_iov<'b, 'a: 'b>(
            &'a self,
            iov_buf: &'b mut [IoSlice<'a>; 2],
        ) -> &'b [IoSlice<'a>] {
            let head = self.head();
            if self.offset < head {
                iov_buf[0] = IoSlice::new(&self.bytes[self.offset..head]);
                &iov_buf[0..1]
            } else if head == 0 {
                iov_buf[0] = IoSlice::new(&self.bytes[self.offset..]);
                &iov_buf[0..1]
            } else {
                let (left, right) = self.bytes.split_at(self.offset);
                iov_buf[0] = IoSlice::new(right);
                iov_buf[1] = IoSlice::new(&left[..head]);
                &iov_buf[0..2]
            }
        }

        pub fn write_int(&mut self, val: i32) {
            self.write_bytes(&val.to_ne_bytes());
        }

        pub fn write_uint(&mut self, val: u32) {
            self.write_bytes(&val.to_ne_bytes());
        }

        pub fn read_int(&mut self) -> i32 {
            let mut buf = [0; 4];
            self.read_bytes(&mut buf);
            i32::from_ne_bytes(buf)
        }

        pub fn read_uint(&mut self) -> u32 {
            let mut buf = [0; 4];
            self.read_bytes(&mut buf);
            u32::from_ne_bytes(buf)
        }

        pub fn read_id(&mut self) -> Option<ObjectId> {
            NonZeroU32::new(self.read_uint()).map(ObjectId)
        }
    }
}
