//! A simple "free list" shared memory allocator

use std::fs::File;
use std::os::unix::io::{BorrowedFd, FromRawFd};

use memmap2::MmapMut;

use wayrs_client::connection::Connection;
use wayrs_client::protocol::*;

use wl_shm::{Format, WlShm};
use wl_shm_pool::WlShmPool;

macro_rules! alloc_id {
    ($self:ident) => {{
        let id = $self.next_id;
        $self.next_id += 1;
        id
    }};
}

/// A simple "free list" shared memory allocator
#[derive(Debug)]
pub struct ShmAlloc {
    pool: WlShmPool,
    len: usize,
    file: File,
    mmap: MmapMut,
    segments: Vec<Segment>,
    next_id: u64,
}

/// Implement this trait on your Wayland state struct to use [`ShmAlloc`]
pub trait ShmAllocState: Sized + 'static {
    /// Get a mutable reference to [`ShmAlloc`].
    ///
    /// This implies that only a single [`ShmAlloc`] can be used.
    fn shm_alloc(&mut self) -> &mut ShmAlloc;
}

#[derive(Debug)]
struct Segment {
    id: u64,
    offset: usize,
    len: usize,
    refcnt: u32,
    buffer: Option<Buffer>,
}

/// A `wl_buffer` with some metadata.
#[derive(Debug, Clone, Copy)]
pub struct Buffer {
    pub id: u64,
    pub wl: WlBuffer,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub format: Format,
}

impl ShmAlloc {
    /// Create new [`ShmAlloc`].
    ///
    /// Only one instance of [`ShmAlloc`] can be practically used (see [`ShmAllocState::shm_alloc`]).
    /// This limitation might get resolved in the future.
    pub fn new<D: ShmAllocState>(
        conn: &mut Connection<D>,
        wl_shm: WlShm,
        initial_len: usize,
    ) -> Self {
        let fd = shmemfdrs::create_shmem(wayrs_client::cstr!("/wayrs_shm_pool"), initial_len);
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
                id: 0,
                offset: 0,
                len: initial_len,
                refcnt: 0,
                buffer: None,
            }],
            next_id: 1,
        }
    }

    /// Allocate a new buffer.
    ///
    /// The underlying memory pool will be resized if needed. Previously released buffers are
    /// reused whenever possible.
    ///
    /// See [`WlShmPool::create_buffer`] for more info.
    pub fn alloc_buffer<D: ShmAllocState>(
        &mut self,
        conn: &mut Connection<D>,
        width: i32,
        height: i32,
        stride: i32,
        format: Format,
    ) -> (Buffer, &mut [u8]) {
        let size = height * stride;

        let segment_index = self.alloc_segment(conn, size as usize, format);
        let segment = &mut self.segments[segment_index];

        let buffer = *segment.buffer.get_or_insert_with(|| {
            let wl = self.pool.create_buffer_with_cb(
                conn,
                segment.offset as i32,
                width,
                height,
                stride,
                format,
                |_conn, state, wl_buffer, event| {
                    let wl_buffer::Event::Release = event;
                    state.shm_alloc().free_buffer(wl_buffer);
                    // We don't destroy the buffer here because it can be reused later
                },
            );
            Buffer {
                id: segment.id,
                wl,
                width,
                height,
                stride,
                format,
            }
        });

        let bytes = &mut self.mmap[segment.offset..][..segment.len];

        (buffer, bytes)
    }

    /// Create a buffer that shares the underlying memory with another buffer.
    ///
    /// `buffer_id` must be the value of [`Buffer::id`] of a buffer that needs to be duplicated.
    ///
    /// Returns `None` if a buffer with `buffer_id` could not be found, either because the id is
    /// wrong or because the buffer has been freed and reused.
    ///
    /// Calling this function for an attached and then released buffer may succeed, but it is not
    /// guaranteed. For reliable results duplicate only non-attached buffers.
    pub fn duplicate_buffer<D: ShmAllocState>(
        &mut self,
        conn: &mut Connection<D>,
        buffer_id: u64,
    ) -> Option<Buffer> {
        let segment = self.segments.iter_mut().find(|s| s.id == buffer_id)?;
        let buffer = segment.buffer?;
        segment.refcnt += 1;
        let wl = self.pool.create_buffer_with_cb(
            conn,
            segment.offset as i32,
            buffer.width,
            buffer.height,
            buffer.stride,
            buffer.format,
            move |conn, state, wl_buffer, event| {
                let wl_buffer::Event::Release = event;
                state
                    .shm_alloc()
                    .segments
                    .iter_mut()
                    .find(|s| s.id == buffer_id)
                    .expect("segment for a released buffer not found")
                    .refcnt -= 1;
                wl_buffer.destroy(conn);
            },
        );
        Some(Buffer { wl, ..buffer })
    }

    /// Call this function only if you created a buffer, did non attach it to any surface and
    /// decided that you will not use it. **Attached buffers are freed automatically.**
    pub fn free_buffer(&mut self, buffer: WlBuffer) {
        for segment in &mut self.segments {
            if let Some(b) = &segment.buffer {
                if b.wl == buffer {
                    assert!(segment.refcnt > 0);
                    segment.refcnt -= 1;
                    break;
                }
            }
        }
    }

    fn merge_segments<D>(&mut self, conn: &mut Connection<D>) {
        let mut i = 0;
        while i + 1 < self.segments.len() {
            if self.segments[i].refcnt != 0 || self.segments[i + 1].refcnt != 0 {
                i += 1;
                continue;
            }

            if let Some(buffer) = self.segments[i].buffer.take() {
                buffer.wl.destroy(conn);
            }
            if let Some(buffer) = self.segments[i + 1].buffer.take() {
                buffer.wl.destroy(conn);
            }

            self.segments[i].len += self.segments[i + 1].len;
            self.segments[i].id = alloc_id!(self);

            self.segments.remove(i + 1);
        }
    }

    fn resize<D>(&mut self, conn: &mut Connection<D>, new_len: usize) {
        if new_len > self.len {
            self.len = new_len;
            self.file.set_len(new_len as u64).unwrap();
            self.pool.resize(conn, new_len as i32);
            self.mmap = unsafe { MmapMut::map_mut(&self.file).expect("memory mapping failed") };
        }
    }

    // Returns segment index, does not resize
    fn try_alloc_in_place<D>(&mut self, conn: &mut Connection<D>, len: usize) -> Option<usize> {
        // Find a segment with exact size
        for (i, segment) in self.segments.iter_mut().enumerate() {
            if segment.refcnt == 0 && segment.len == len {
                if let Some(buffer) = segment.buffer.take() {
                    buffer.wl.destroy(conn);
                }
                segment.refcnt = 1;
                return Some(i);
            }
        }
        // Find a segment large enough
        for (i, segment) in self.segments.iter_mut().enumerate() {
            if segment.refcnt == 0 && segment.len > len {
                let offset = segment.offset;
                if let Some(buffer) = segment.buffer.take() {
                    buffer.wl.destroy(conn);
                }
                segment.offset += len;
                segment.len -= len;
                segment.id = alloc_id!(self);
                self.segments.insert(
                    i,
                    Segment {
                        id: alloc_id!(self),
                        offset,
                        len,
                        refcnt: 1,
                        buffer: None,
                    },
                );
                return Some(i);
            }
        }
        None
    }

    // Returns segment index
    fn alloc_segment<D>(&mut self, conn: &mut Connection<D>, len: usize, format: Format) -> usize {
        // Find a segment with exact size and a matching buffer
        for (i, segment) in self.segments.iter_mut().enumerate() {
            if segment.refcnt == 0 && segment.len == len {
                if let Some(b) = segment.buffer {
                    if b.format == format {
                        segment.refcnt = 1;
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
            Some(segment) if segment.refcnt == 0 => {
                if let Some(buffer) = segment.buffer.take() {
                    buffer.wl.destroy(conn);
                }
                let new_size = self.len + len - segment.len;
                segment.len = len;
                segment.refcnt = 1;
                segment.id = alloc_id!(self);
                self.resize(conn, new_size);
            }
            _ => {
                let offset = self.len;
                self.resize(conn, self.len + len);
                self.segments.push(Segment {
                    id: alloc_id!(self),
                    offset,
                    len,
                    refcnt: 1,
                    buffer: None,
                });
            }
        }

        self.segments.len() - 1
    }
}
