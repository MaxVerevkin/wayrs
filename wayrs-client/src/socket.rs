use std::env;
use std::ffi::CString;
use std::io::{self, IoSlice, IoSliceMut};
use std::num::NonZeroU32;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use nix::sys::socket::{self, ControlMessage, ControlMessageOwned};

use crate::interface::Interface;
use crate::object::{Object, ObjectId};
use crate::wire::{ArgType, ArgValue, Fixed, Message, MessageHeader};
use crate::{ConnectError, IoMode};

pub const BYTES_OUT_LEN: usize = 4096;
pub const BYTES_IN_LEN: usize = BYTES_OUT_LEN * 2;
pub const FDS_OUT_LEN: usize = 28;
pub const FDS_IN_LEN: usize = FDS_OUT_LEN * 2;

pub struct BufferedSocket {
    socket: UnixStream,
    bytes_in: ArrayBuffer<u8, BYTES_IN_LEN>,
    bytes_out: ArrayBuffer<u8, BYTES_OUT_LEN>,
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
            bytes_in: ArrayBuffer::new(),
            bytes_out: ArrayBuffer::new(),
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
        if (size as usize) < self.bytes_out.get_writable().len()
            || fds_cnt < self.fds_out.get_writable().len()
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
        while self.bytes_in.get_readable().len() < MessageHeader::size() as usize {
            self.fill_incoming_buf(mode)?;
        }

        let raw = self.bytes_in.get_readable();
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
        while header.size as usize > self.bytes_in.get_readable().len()
            || fds_cnt > self.fds_in.get_readable().len()
        {
            self.fill_incoming_buf(mode)?;
        }

        // Consume header
        self.bytes_in.consume(MessageHeader::size() as usize);

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
        if self.bytes_out.get_readable().is_empty() {
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

        let sent = socket::sendmsg::<()>(
            self.socket.as_raw_fd(),
            &[IoSlice::new(self.bytes_out.get_readable())],
            cmsgs,
            flags,
            None,
        )?;

        for fd in self.fds_out.get_readable() {
            let _ = nix::unistd::close(*fd);
        }

        // Does this have to be thue?
        assert_eq!(sent, self.bytes_out.get_readable().len());

        self.bytes_out.clear();
        self.fds_out.clear();

        Ok(())
    }
}

impl BufferedSocket {
    fn fill_incoming_buf(&mut self, mode: IoMode) -> io::Result<()> {
        self.bytes_in.relocate();
        self.fds_in.relocate();
        if self.bytes_in.get_writable().is_empty() && self.fds_in.get_writable().is_empty() {
            return Ok(());
        }

        let mut cmsg = nix::cmsg_space!([RawFd; FDS_OUT_LEN]);

        let mut flags = socket::MsgFlags::MSG_CMSG_CLOEXEC | socket::MsgFlags::MSG_NOSIGNAL;
        if mode == IoMode::NonBlocking {
            flags |= socket::MsgFlags::MSG_DONTWAIT;
        }

        let msg = socket::recvmsg::<()>(
            self.socket.as_raw_fd(),
            &mut [IoSliceMut::new(self.bytes_in.get_writable())],
            Some(&mut cmsg),
            flags,
        )?;

        for cmsg in msg.cmsgs() {
            if let ControlMessageOwned::ScmRights(fds) = cmsg {
                self.fds_in.write_exact(&fds);
            }
        }

        let read = msg.bytes;
        self.bytes_in.advance(read);

        if read == 0 {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "server disconnected",
            ))
        } else {
            Ok(())
        }
    }

    fn send_array(&mut self, array: &[u8]) {
        let len = array.len() as u32;

        self.bytes_out.write_uint(len);
        self.bytes_out.write_exact(array);

        let padding = ((4 - (len % 4)) % 4) as usize;
        self.bytes_out.write_exact(&[0, 0, 0][..padding]);
    }

    fn recv_array(&mut self) -> Vec<u8> {
        let len = self.bytes_in.read_uint() as usize;

        let mut buf = vec![0; len];
        self.bytes_in.read_exact(&mut buf);

        let padding = (4 - (len % 4)) % 4;
        self.bytes_in.consume(padding);

        buf
    }

    fn recv_string_with_len(&mut self, len: u32) -> CString {
        let mut buf = vec![0; len as usize];
        self.bytes_in.read_exact(&mut buf);

        let padding = (4 - (len % 4)) % 4;
        self.bytes_in.consume(padding as usize);

        CString::from_vec_with_nul(buf).expect("received string with internal null bytes")
    }

    fn recv_string(&mut self) -> CString {
        let len = self.bytes_in.read_uint();
        self.recv_string_with_len(len)
    }
}

struct ArrayBuffer<T, const N: usize> {
    bytes: Box<[T; N]>,
    offset: usize,
    len: usize,
}

impl<T: Default + Copy, const N: usize> ArrayBuffer<T, N> {
    fn new() -> Self {
        Self {
            bytes: Box::new([T::default(); N]),
            offset: 0,
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.offset = 0;
        self.len = 0;
    }

    fn get_writable(&mut self) -> &mut [T] {
        &mut self.bytes[(self.offset + self.len)..]
    }

    fn get_readable(&self) -> &[T] {
        &self.bytes[self.offset..][..self.len]
    }

    fn consume(&mut self, cnt: usize) {
        assert!(cnt <= self.len);
        self.offset += cnt;
        self.len -= cnt;
    }

    fn advance(&mut self, cnt: usize) {
        assert!(self.offset + self.len + cnt <= N);
        self.len += cnt;
    }

    fn relocate(&mut self) {
        if self.len > 0 && self.offset > 0 {
            self.bytes
                .copy_within(self.offset..(self.offset + self.len), 0);
        }
        self.offset = 0;
    }

    fn write_one(&mut self, elem: T) {
        let writable = self.get_writable();
        assert!(!writable.is_empty());
        writable[0] = elem;
        self.advance(1);
    }

    fn read_one(&mut self) -> T {
        let readable = self.get_readable();
        assert!(!readable.is_empty());
        let elem = readable[0];
        self.consume(1);
        elem
    }

    fn write_exact(&mut self, src: &[T]) {
        let writable = &mut self.get_writable()[..src.len()];
        writable.copy_from_slice(src);
        self.advance(src.len());
    }

    fn read_exact(&mut self, dst: &mut [T]) {
        let readable = &self.get_readable()[..dst.len()];
        dst.copy_from_slice(readable);
        self.consume(dst.len());
    }
}

impl<const N: usize> ArrayBuffer<u8, N> {
    fn write_int(&mut self, int: i32) {
        self.write_exact(&int.to_ne_bytes());
    }

    fn write_uint(&mut self, uint: u32) {
        self.write_exact(&uint.to_ne_bytes());
    }

    fn read_int(&mut self) -> i32 {
        let mut buf = [0; 4];
        self.read_exact(&mut buf);
        i32::from_ne_bytes(buf)
    }

    fn read_uint(&mut self) -> u32 {
        let mut buf = [0; 4];
        self.read_exact(&mut buf);
        u32::from_ne_bytes(buf)
    }

    fn read_id(&mut self) -> Option<ObjectId> {
        NonZeroU32::new(self.read_uint()).map(ObjectId)
    }
}
