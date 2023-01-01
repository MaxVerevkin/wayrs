use std::fs::File;
use std::os::unix::io::{BorrowedFd, FromRawFd};

use memmap2::MmapMut;

use wayrs_client::connection::Connection;
use wayrs_client::protocol::*;
use wayrs_client::proxy::{Dispatch, Dispatcher};

use wl_shm::{Format, WlShm};
use wl_shm_pool::WlShmPool;

#[derive(Debug)]
pub struct ShmAlloc {
    pool: WlShmPool,
    len: usize,
    file: File,
    mmap: MmapMut,
    segments: Vec<Segment>,
}

#[derive(Debug)]
struct Segment {
    offset: usize,
    len: usize,
    free: bool,
    buffer: Option<Buffer>,
}

#[derive(Debug, Clone, Copy)]
struct Buffer {
    wl: WlBuffer,
    format: Format,
}

impl ShmAlloc {
    pub fn new<D: Dispatch<WlShmPool>>(
        conn: &mut Connection<D>,
        wl_shm: WlShm,
        initial_len: usize,
    ) -> Self {
        let fd = shmemfdrs::create_shmem(wayrs_client::cstr!("/ramp-buffer"), initial_len);
        let file = unsafe { File::from_raw_fd(fd) };
        let mmap = unsafe { MmapMut::map_mut(&file).expect("memory mapping failed") };

        let fd_dup = unsafe {
            BorrowedFd::borrow_raw(fd)
                .try_clone_to_owned()
                .expect("could not duplicate fd")
        };
        let pool = wl_shm.create_pool(conn, fd_dup, initial_len as i32);

        Self {
            pool,
            len: initial_len,
            file,
            mmap,
            segments: vec![Segment {
                offset: 0,
                len: initial_len,
                free: true,
                buffer: None,
            }],
        }
    }

    pub fn alloc_buffer<D: Dispatch<WlBuffer>>(
        &mut self,
        conn: &mut Connection<D>,
        width: i32,
        height: i32,
        stride: i32,
        format: Format,
    ) -> (WlBuffer, &mut [u8]) {
        let size = height * stride;

        let segment_index = self.alloc_segment(conn, size as usize, format);
        let segment = &mut self.segments[segment_index];

        let buffer = segment.buffer.get_or_insert_with(|| Buffer {
            wl: self
                .pool
                .create_buffer(conn, segment.offset as i32, width, height, stride, format),
            format,
        });

        let bytes = &mut self.mmap[segment.offset..][..segment.len];

        (buffer.wl, bytes)
    }

    pub fn free_buffer(&mut self, buffer: WlBuffer) {
        for segment in &mut self.segments {
            if let Some(b) = &segment.buffer {
                if b.wl == buffer {
                    segment.free = true;
                    break;
                }
            }
        }
    }

    fn merge_segments<D: Dispatcher>(&mut self, conn: &mut Connection<D>) {
        let mut i = 0;
        while i + 1 < self.segments.len() {
            if self.segments[i].free && self.segments[i + 1].free {
                if let Some(buffer) = self.segments[i].buffer.take() {
                    buffer.wl.destroy(conn);
                }
                if let Some(buffer) = self.segments[i + 1].buffer.take() {
                    buffer.wl.destroy(conn);
                }
                self.segments[i].len += self.segments[i + 1].len;
                self.segments.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }

    fn resize<D: Dispatcher>(&mut self, conn: &mut Connection<D>, new_len: usize) {
        if new_len > self.len {
            self.len = new_len;
            self.file.set_len(new_len as u64).unwrap();
            self.pool.resize(conn, new_len as i32);
            self.mmap = unsafe { MmapMut::map_mut(&self.file).expect("memory mapping failed") };
        }
    }

    // Returns segment index, does not resize
    fn try_alloc_in_place<D: Dispatcher>(
        &mut self,
        conn: &mut Connection<D>,
        len: usize,
    ) -> Option<usize> {
        // Find a segment with exact size
        for (i, segment) in self.segments.iter_mut().enumerate() {
            if segment.free && segment.len == len {
                if let Some(buffer) = segment.buffer.take() {
                    buffer.wl.destroy(conn);
                }
                segment.free = false;
                return Some(i);
            }
        }
        // Find a segment large enough
        for (i, segment) in self.segments.iter_mut().enumerate() {
            if segment.free && segment.len > len {
                let offset = segment.offset;
                if let Some(buffer) = segment.buffer.take() {
                    buffer.wl.destroy(conn);
                }
                segment.offset += len;
                segment.len -= len;
                self.segments.insert(
                    i,
                    Segment {
                        offset,
                        len,
                        free: false,
                        buffer: None,
                    },
                );
                return Some(i);
            }
        }
        None
    }

    // Returns segment index
    fn alloc_segment<D: Dispatcher>(
        &mut self,
        conn: &mut Connection<D>,
        len: usize,
        format: Format,
    ) -> usize {
        // Find a segment with exact size and a matching buffer
        for (i, segment) in self.segments.iter_mut().enumerate() {
            if segment.free && segment.len == len {
                if let Some(b) = segment.buffer {
                    if b.format == format {
                        segment.free = false;
                        return i;
                    }
                }
            }
        }

        if let Some(index) = self.try_alloc_in_place(conn, len) {
            return index;
        }
        self.merge_segments(conn);
        if let Some(index) = self.try_alloc_in_place(conn, len) {
            return index;
        }

        match self.segments.last_mut() {
            Some(segment) if segment.free => {
                if let Some(buffer) = segment.buffer.take() {
                    buffer.wl.destroy(conn);
                }
                let new_size = self.len + len - segment.len;
                segment.len = len;
                segment.free = false;
                self.resize(conn, new_size);
            }
            _ => {
                let offset = self.len;
                self.resize(conn, self.len + len);
                self.segments.push(Segment {
                    offset,
                    len,
                    free: false,
                    buffer: None,
                });
            }
        }

        self.segments.len() - 1
    }
}
