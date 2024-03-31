use std::io::{IoSlice, IoSliceMut};
use std::num::NonZeroU32;

use crate::ObjectId;

pub struct RingBuffer {
    bytes: Box<[u8]>,
    offset: usize,
    len: usize,
}

impl RingBuffer {
    pub fn new(size: usize) -> Self {
        Self {
            bytes: Box::from(vec![0; size]),
            offset: 0,
            len: 0,
        }
    }

    pub fn move_head(&mut self, n: usize) {
        self.len += n;
    }

    pub fn move_tail(&mut self, n: usize) {
        self.offset = (self.offset + n) % self.bytes.len();
        self.len = self.len.checked_sub(n).unwrap();
    }

    pub fn readable_len(&self) -> usize {
        self.len
    }

    pub fn writable_len(&self) -> usize {
        self.bytes.len() - self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == self.bytes.len()
    }

    fn head(&self) -> usize {
        (self.offset + self.len) % self.bytes.len()
    }

    pub fn write_bytes(&mut self, data: &[u8]) {
        assert!(self.writable_len() >= data.len());

        let head = self.head();
        if head + data.len() <= self.bytes.len() {
            self.bytes[head..][..data.len()].copy_from_slice(data);
        } else {
            let size = self.bytes.len() - head;
            let rest = data.len() - size;
            self.bytes[head..][..size].copy_from_slice(&data[..size]);
            self.bytes[..rest].copy_from_slice(&data[size..]);
        }

        self.move_head(data.len());
    }

    pub fn peek_bytes(&mut self, buf: &mut [u8]) {
        assert!(self.readable_len() >= buf.len());

        if self.offset + buf.len() <= self.bytes.len() {
            buf.copy_from_slice(&self.bytes[self.offset..][..buf.len()]);
        } else {
            let size = self.bytes.len() - self.offset;
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
            iov_buf[0] = IoSliceMut::new(&mut self.bytes);
            &mut iov_buf[0..1]
        } else if head < self.offset {
            iov_buf[0] = IoSliceMut::new(&mut self.bytes[head..self.offset]);
            &mut iov_buf[0..1]
        } else if self.offset == 0 {
            iov_buf[0] = IoSliceMut::new(&mut self.bytes[head..]);
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
