use std::env;
use std::ffi::CString;
use std::io::{self, IoSlice, IoSliceMut};
use std::num::NonZeroU32;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use nix::sys::socket::{self, ControlMessage, ControlMessageOwned};

use crate::interface::Interface;
use crate::object::{Object, ObjectId};
use crate::wire::{ArgType, ArgValue, Fixed, Message, MessageHeader};
use crate::{ConnectError, IoMode};

use buf::{ArrayBuffer, RingBuffer};

pub const BYTES_OUT_LEN: usize = 4096;
pub const BYTES_IN_LEN: usize = BYTES_OUT_LEN * 2;
pub const FDS_OUT_LEN: usize = 28;
pub const FDS_IN_LEN: usize = FDS_OUT_LEN * 2;

pub struct BufferedSocket {
    socket: UnixStream,
    bytes_in: RingBuffer<BYTES_IN_LEN>,
    bytes_out: RingBuffer<BYTES_OUT_LEN>,
    fds_in: ArrayBuffer<RawFd, FDS_IN_LEN>,
    fds_out: ArrayBuffer<RawFd, FDS_OUT_LEN>,
}

impl AsRawFd for BufferedSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

pub struct SendMessageError {
    pub msg: Message,
    pub err: io::Error,
}

impl BufferedSocket {
    pub fn connect() -> Result<Self, ConnectError> {
        let runtime_dir = env::var_os("XDG_RUNTIME_DIR").ok_or(ConnectError::NotEnoughEnvVars)?;
        let wayland_disp = env::var_os("WAYLAND_DISPLAY").ok_or(ConnectError::NotEnoughEnvVars)?;

        let mut path = PathBuf::new();
        path.push(runtime_dir);
        path.push(wayland_disp);

        Ok(Self {
            socket: UnixStream::connect(path)?,
            bytes_in: RingBuffer::new(),
            bytes_out: RingBuffer::new(),
            fds_in: ArrayBuffer::new(),
            fds_out: ArrayBuffer::new(),
        })
    }

    /// Write a single Wayland message into the intevnal buffer.
    ///
    /// Flushes the buffer if neccessary. On failure, ownership of the message is returned.
    ///
    /// # Panics
    ///
    /// This function panics if the message size is larger than `BYTES_OUT_LEN` or it contains more
    /// than `FDS_OUT_LEN` file descriptors.
    pub fn write_message(&mut self, msg: Message, mode: IoMode) -> Result<(), SendMessageError> {
        // Calc size
        let size = MessageHeader::size() + msg.args.iter().map(ArgValue::size).sum::<u16>();
        let fds_cnt = msg
            .args
            .iter()
            .filter(|arg| matches!(arg, ArgValue::Fd(_)))
            .count();

        // Check size and flush if neccessary
        assert!(size as usize <= BYTES_OUT_LEN);
        assert!(fds_cnt <= FDS_OUT_LEN);
        if (size as usize) > self.bytes_out.writable_len()
            || fds_cnt > self.fds_out.get_writable().len()
        {
            if let Err(err) = self.flush(mode) {
                return Err(SendMessageError { msg, err });
            }
        }

        // Header
        self.bytes_out.write_uint(msg.header.object_id.0.get());
        self.bytes_out
            .write_uint((size as u32) << 16 | msg.header.opcode as u32);

        // Args
        for arg in msg.args.into_iter() {
            match arg {
                ArgValue::Uint(x) => self.bytes_out.write_uint(x),
                ArgValue::Int(x) | ArgValue::Fixed(Fixed(x)) => self.bytes_out.write_int(x),
                ArgValue::Object(ObjectId(x))
                | ArgValue::OptObject(Some(ObjectId(x)))
                | ArgValue::NewIdRequest(ObjectId(x)) => self.bytes_out.write_uint(x.get()),
                ArgValue::OptObject(None) | ArgValue::OptString(None) => {
                    self.bytes_out.write_uint(0)
                }
                ArgValue::AnyNewIdRequest(obj) => {
                    self.send_array(obj.interface.name.to_bytes_with_nul());
                    self.bytes_out.write_uint(obj.version);
                    self.bytes_out.write_uint(obj.id.0.get());
                }
                ArgValue::String(string) | ArgValue::OptString(Some(string)) => {
                    self.send_array(string.to_bytes_with_nul())
                }
                ArgValue::Array(array) => self.send_array(&array),
                ArgValue::Fd(fd) => self.fds_out.write_one(fd.into_raw_fd()),
                ArgValue::NewIdEvent(_) => panic!("NewIdEvent in request"),
            }
        }

        Ok(())
    }

    pub fn peek_message_header(&mut self, mode: IoMode) -> io::Result<MessageHeader> {
        while self.bytes_in.readable_len() < MessageHeader::size() as usize {
            self.fill_incoming_buf(mode)?;
        }

        let mut raw = [0; MessageHeader::size() as usize];
        self.bytes_in.peek_bytes(&mut raw);
        let object_id = u32::from_ne_bytes(raw[0..4].try_into().unwrap());
        let size_and_opcode = u32::from_ne_bytes(raw[4..8].try_into().unwrap());

        Ok(MessageHeader {
            object_id: ObjectId(NonZeroU32::new(object_id).expect("received event for null id")),
            size: ((size_and_opcode & 0xFFFF_0000) >> 16) as u16,
            opcode: (size_and_opcode & 0x0000_FFFF) as u16,
        })
    }

    pub fn recv_message(
        &mut self,
        header: MessageHeader,
        iface: &'static Interface,
        version: u32,
        mode: IoMode,
    ) -> io::Result<Message> {
        let signature = iface
            .events
            .get(header.opcode as usize)
            .expect("incorrect opcode")
            .signature;

        // Check size and fill buffer if necessary
        let fds_cnt = signature
            .iter()
            .filter(|arg| matches!(arg, ArgType::Fd))
            .count();
        assert!(header.size as usize <= BYTES_IN_LEN);
        assert!(fds_cnt <= FDS_IN_LEN);
        while header.size as usize > self.bytes_in.readable_len()
            || fds_cnt > self.fds_in.get_readable().len()
        {
            self.fill_incoming_buf(mode)?;
        }

        // Consume header
        self.bytes_in.move_tail(MessageHeader::size() as usize);

        let args = signature
            .iter()
            .map(|arg_type| match arg_type {
                ArgType::Int => ArgValue::Int(self.bytes_in.read_int()),
                ArgType::Uint => ArgValue::Uint(self.bytes_in.read_uint()),
                ArgType::Fixed => ArgValue::Fixed(Fixed(self.bytes_in.read_int())),
                ArgType::Object => {
                    ArgValue::Object(self.bytes_in.read_id().expect("unexpected null object id"))
                }
                ArgType::OptObject => ArgValue::OptObject(self.bytes_in.read_id()),
                ArgType::NewId(interface) => ArgValue::NewIdEvent(Object {
                    id: self.bytes_in.read_id().expect("unexpected null new_id"),
                    interface,
                    version,
                }),
                ArgType::AnyNewId => unimplemented!(),
                ArgType::String => ArgValue::String(self.recv_string()),
                ArgType::OptString => ArgValue::OptString(match self.bytes_in.read_uint() {
                    0 => None,
                    len => Some(self.recv_string_with_len(len)),
                }),
                ArgType::Array => ArgValue::Array(self.recv_array()),
                ArgType::Fd => {
                    let fd = self.fds_in.read_one();
                    assert_ne!(fd, -1);
                    ArgValue::Fd(unsafe { OwnedFd::from_raw_fd(fd) })
                }
            })
            .collect();

        Ok(Message { header, args })
    }

    pub fn flush(&mut self, mode: IoMode) -> io::Result<()> {
        if self.bytes_out.is_empty() && self.fds_out.get_readable().is_empty() {
            return Ok(());
        }

        let mut flags = socket::MsgFlags::MSG_NOSIGNAL;
        if mode == IoMode::NonBlocking {
            flags |= socket::MsgFlags::MSG_DONTWAIT;
        }

        let b;
        let cmsgs: &[ControlMessage] = match self.fds_out.get_readable() {
            [] => &[],
            fds => {
                b = [ControlMessage::ScmRights(fds)];
                &b
            }
        };

        let mut iov_buf = [IoSlice::new(&[]), IoSlice::new(&[])];
        let iov = self.bytes_out.get_readable_iov(&mut iov_buf);
        let sent = socket::sendmsg::<()>(self.socket.as_raw_fd(), iov, cmsgs, flags, None)?;

        for fd in self.fds_out.get_readable() {
            let _ = nix::unistd::close(*fd);
        }

        // Does this have to be true?
        assert_eq!(sent, self.bytes_out.readable_len());

        self.bytes_out.clear();
        self.fds_out.clear();

        Ok(())
    }
}

impl BufferedSocket {
    fn fill_incoming_buf(&mut self, mode: IoMode) -> io::Result<()> {
        self.fds_in.relocate();
        if self.bytes_in.is_full() && self.fds_in.get_writable().is_empty() {
            return Ok(());
        }

        let mut cmsg = nix::cmsg_space!([RawFd; FDS_OUT_LEN]);

        let mut flags = socket::MsgFlags::MSG_CMSG_CLOEXEC | socket::MsgFlags::MSG_NOSIGNAL;
        if mode == IoMode::NonBlocking {
            flags |= socket::MsgFlags::MSG_DONTWAIT;
        }

        let mut iov_buf = [IoSliceMut::new(&mut []), IoSliceMut::new(&mut [])];
        let iov = self.bytes_in.get_writeable_iov(&mut iov_buf);
        let msg = socket::recvmsg::<()>(self.socket.as_raw_fd(), iov, Some(&mut cmsg), flags)?;

        for cmsg in msg.cmsgs() {
            if let ControlMessageOwned::ScmRights(fds) = cmsg {
                self.fds_in.extend(&fds);
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

    pub struct ArrayBuffer<T, const N: usize> {
        bytes: Box<[T; N]>,
        offset: usize,
        len: usize,
    }

    impl<T: Default + Copy, const N: usize> ArrayBuffer<T, N> {
        pub fn new() -> Self {
            Self {
                bytes: Box::new([T::default(); N]),
                offset: 0,
                len: 0,
            }
        }

        pub fn clear(&mut self) {
            self.offset = 0;
            self.len = 0;
        }

        pub fn get_writable(&mut self) -> &mut [T] {
            &mut self.bytes[(self.offset + self.len)..]
        }

        pub fn get_readable(&self) -> &[T] {
            &self.bytes[self.offset..][..self.len]
        }

        pub fn consume(&mut self, cnt: usize) {
            assert!(cnt <= self.len);
            self.offset += cnt;
            self.len -= cnt;
        }

        pub fn advance(&mut self, cnt: usize) {
            assert!(self.offset + self.len + cnt <= N);
            self.len += cnt;
        }

        pub fn relocate(&mut self) {
            if self.len > 0 && self.offset > 0 {
                self.bytes
                    .copy_within(self.offset..(self.offset + self.len), 0);
            }
            self.offset = 0;
        }

        pub fn write_one(&mut self, elem: T) {
            let writable = self.get_writable();
            assert!(!writable.is_empty());
            writable[0] = elem;
            self.advance(1);
        }

        pub fn read_one(&mut self) -> T {
            let readable = self.get_readable();
            assert!(!readable.is_empty());
            let elem = readable[0];
            self.consume(1);
            elem
        }

        pub fn extend(&mut self, src: &[T]) {
            let writable = &mut self.get_writable()[..src.len()];
            writable.copy_from_slice(src);
            self.advance(src.len());
        }
    }

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
