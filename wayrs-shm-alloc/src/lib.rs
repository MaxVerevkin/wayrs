//! A simple "free list" shared memory allocator

use std::fs::File;
use std::os::unix::io::{BorrowedFd, FromRawFd};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use memmap2::MmapMut;

use wayrs_client::connection::Connection;
use wayrs_client::protocol::*;

use wl_shm::{Format, WlShm};
use wl_shm_pool::WlShmPool;

/// A simple "free list" shared memory allocator
#[derive(Debug)]
pub struct ShmAlloc {
    wl_shm: WlShm,
    pool: Option<InitShmPoll>,
}

#[derive(Debug)]
struct InitShmPoll {
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
    refcnt: Arc<AtomicU32>,
    buffer: Option<(WlBuffer, BufferSpec)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferSpec {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: Format,
}

impl BufferSpec {
    pub fn size(&self) -> usize {
        self.stride as usize * self.height as usize
    }
}

/// A `wl_buffer` with some metadata.
#[derive(Debug)]
pub struct Buffer {
    spec: BufferSpec,
    wl: WlBuffer,
    refcnt: Arc<AtomicU32>,
    wl_shm_pool: WlShmPool,
    offset: usize,
}

impl ShmAlloc {
    /// Create new [`ShmAlloc`].
    pub fn new(wl_shm: WlShm) -> Self {
        Self { wl_shm, pool: None }
    }

    /// Allocate a new buffer.
    ///
    /// The underlying memory pool will be resized if needed. Previously released buffers are
    /// reused whenever possible.
    ///
    /// See [`WlShmPool::create_buffer`] for more info.
    pub fn alloc_buffer<D>(
        &mut self,
        conn: &mut Connection<D>,
        spec: BufferSpec,
    ) -> (Buffer, &mut [u8]) {
        self.pool
            .get_or_insert_with(|| InitShmPoll::new(conn, self.wl_shm, spec.size()))
            .alloc_buffer(conn, spec)
    }
}

impl Buffer {
    /// Get the underlying `wl_buffer`.
    ///
    /// This `wl_buffer` must be attached to exactly one surface, otherwise the memory may be
    /// leaked or a panic may occur during [`Connection::dispatch_events`].
    #[must_use = "memory is leaked if wl_buffer is not attached"]
    pub fn into_wl_buffer(self) -> WlBuffer {
        let wl = self.wl;
        std::mem::forget(self);
        wl
    }

    /// Create a `wl_buffer` that shares the same spec and underlying memory as `self`.
    ///
    /// This `wl_buffer` must be attached to exactly one surface, otherwise the memory may be
    /// leaked or a panic may occur during [`Connection::dispatch_events`] or `self`'s drop.
    ///
    /// This method is usefull if you want to attach the same buffer to a number of surfaces. In
    /// fact, this is the only correct way to do it unisg this library.
    #[must_use = "memory is leaked if wl_buffer is not attached"]
    pub fn duplicate<D>(&self, conn: &mut Connection<D>) -> WlBuffer {
        self.refcnt.fetch_add(1, Ordering::AcqRel);
        let refcnt = Arc::clone(&self.refcnt);
        self.wl_shm_pool.create_buffer_with_cb(
            conn,
            self.offset as i32,
            self.spec.width as i32,
            self.spec.height as i32,
            self.spec.stride as i32,
            self.spec.format,
            move |conn, _state, wl_buffer, event| {
                let wl_buffer::Event::Release = event;
                assert!(refcnt.fetch_sub(1, Ordering::AcqRel) > 0);
                wl_buffer.destroy(conn);
            },
        )
    }

    /// Get the spec of this buffer
    pub fn spec(&self) -> BufferSpec {
        self.spec
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        assert!(self.refcnt.fetch_sub(1, Ordering::AcqRel) > 0);
    }
}

impl InitShmPoll {
    fn new<D>(conn: &mut Connection<D>, wl_shm: WlShm, size: usize) -> InitShmPoll {
        let fd = shmemfdrs::create_shmem(wayrs_client::cstr!("/wayrs_shm_pool"), size);
        let file = unsafe { File::from_raw_fd(fd) };
        let mmap = unsafe { MmapMut::map_mut(&file).expect("memory mapping failed") };

        let fd_dup = unsafe {
            BorrowedFd::borrow_raw(fd)
                .try_clone_to_owned()
                .expect("could not duplicate fd")
        };
        let pool = wl_shm.create_pool(conn, fd_dup, size as i32);

        Self {
            pool,
            len: size,
            file,
            mmap,
            segments: vec![Segment {
                offset: 0,
                len: size,
                refcnt: Arc::new(AtomicU32::new(0)),
                buffer: None,
            }],
        }
    }

    fn alloc_buffer<D>(
        &mut self,
        conn: &mut Connection<D>,
        spec: BufferSpec,
    ) -> (Buffer, &mut [u8]) {
        let size = spec.height * spec.stride;

        let segment_index = self.alloc_segment(conn, size as usize, spec);
        let segment = &mut self.segments[segment_index];

        let (wl, spec) = *segment.buffer.get_or_insert_with(|| {
            let seg_refcnt = Arc::clone(&segment.refcnt);
            let wl = self.pool.create_buffer_with_cb(
                conn,
                segment.offset as i32,
                spec.width as i32,
                spec.height as i32,
                spec.stride as i32,
                spec.format,
                move |_conn, _state, _wl_buffer, event| {
                    let wl_buffer::Event::Release = event;
                    assert!(seg_refcnt.fetch_sub(1, Ordering::SeqCst) > 0);
                    // We don't destroy the buffer here because it can be reused later
                },
            );
            (wl, spec)
        });

        (
            Buffer {
                spec,
                wl,
                refcnt: Arc::clone(&segment.refcnt),
                wl_shm_pool: self.pool,
                offset: segment.offset,
            },
            &mut self.mmap[segment.offset..][..segment.len],
        )
    }

    fn defragment<D>(&mut self, conn: &mut Connection<D>) {
        let mut i = 0;
        while i + 1 < self.segments.len() {
            // `refcnt`s are only incremented from Self's methods. Since we have `&mut self`,
            // `refcnt`s can only decrease during the execution of this function.
            if self.segments[i].refcnt.load(Ordering::SeqCst) != 0
                || self.segments[i + 1].refcnt.load(Ordering::SeqCst) != 0
            {
                i += 1;
                continue;
            }

            if let Some(buffer) = self.segments[i].buffer.take() {
                buffer.0.destroy(conn);
            }
            if let Some(buffer) = self.segments[i + 1].buffer.take() {
                buffer.0.destroy(conn);
            }

            self.segments[i].len += self.segments[i + 1].len;

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
    fn try_alloc_in_place<D>(
        &mut self,
        conn: &mut Connection<D>,
        len: usize,
        spec: BufferSpec,
    ) -> Option<usize> {
        fn take_if_free(s: &Segment) -> bool {
            s.refcnt
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        }

        // Find a segment with exact size
        if let Some((i, segment)) = self
            .segments
            .iter_mut()
            .enumerate()
            .filter(|(_, s)| s.len == len)
            .find(|(_, s)| take_if_free(s))
        {
            if let Some(buffer) = &segment.buffer {
                if buffer.1 != spec {
                    buffer.0.destroy(conn);
                    segment.buffer = None;
                }
            }
            return Some(i);
        }

        // Find a segment large enough
        if let Some((i, segment)) = self
            .segments
            .iter_mut()
            .enumerate()
            .filter(|(_, s)| s.len > len)
            .find(|(_, s)| take_if_free(s))
        {
            if let Some(buffer) = segment.buffer.take() {
                buffer.0.destroy(conn);
            }
            let extra = segment.len - len;
            let offset = segment.offset + len;
            segment.len = len;
            self.segments.insert(
                i + 1,
                Segment {
                    offset,
                    len: extra,
                    refcnt: Arc::new(AtomicU32::new(0)),
                    buffer: None,
                },
            );
            return Some(i);
        }

        None
    }

    // Returns segment index
    fn alloc_segment<D>(
        &mut self,
        conn: &mut Connection<D>,
        len: usize,
        spec: BufferSpec,
    ) -> usize {
        if let Some(index) = self.try_alloc_in_place(conn, len, spec) {
            return index;
        }

        self.defragment(conn);
        if let Some(index) = self.try_alloc_in_place(conn, len, spec) {
            return index;
        }

        match self.segments.last_mut() {
            Some(segment)
                if segment
                    .refcnt
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok() =>
            {
                if let Some(buffer) = segment.buffer.take() {
                    buffer.0.destroy(conn);
                }
                let new_size = self.len + len - segment.len;
                segment.len = len;
                self.resize(conn, new_size);
            }
            _ => {
                let offset = self.len;
                self.resize(conn, self.len + len);
                self.segments.push(Segment {
                    offset,
                    len,
                    refcnt: Arc::new(AtomicU32::new(1)),
                    buffer: None,
                });
            }
        }

        self.segments.len() - 1
    }
}
