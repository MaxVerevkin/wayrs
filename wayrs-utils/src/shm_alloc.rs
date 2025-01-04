//! A simple "free list" shared memory allocator

use std::fs::File;
use std::io;
use std::os::fd::AsFd;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use memmap2::MmapMut;

use wayrs_client::global::BindError;
use wayrs_client::object::Proxy;
use wayrs_client::protocol::*;
use wayrs_client::Connection;

/// A simple "free list" shared memory allocator
#[derive(Debug)]
pub struct ShmAlloc {
    state: ShmAllocState,
}

#[derive(Debug)]
enum ShmAllocState {
    Uninit(WlShm),
    Init(InitShmPool),
}

#[derive(Debug)]
struct InitShmPool {
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
    pub format: wl_shm::Format,
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
    /// Bind `wl_shm` and create new [`ShmAlloc`].
    pub fn bind<D>(conn: &mut Connection<D>) -> Result<Self, BindError> {
        Ok(Self::new(conn.bind_singleton(1..=2)?))
    }

    /// Create new [`ShmAlloc`].
    ///
    /// This function takes the ownership of `wl_shm` and destroys it when it is no longer used.
    pub fn new(wl_shm: WlShm) -> Self {
        Self {
            state: ShmAllocState::Uninit(wl_shm),
        }
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
    ) -> io::Result<(Buffer, &mut [u8])> {
        // Note: `if let` does not work here because borrow checker is to dumb
        if matches!(&self.state, ShmAllocState::Init(_)) {
            let ShmAllocState::Init(pool) = &mut self.state else {
                unreachable!()
            };
            return pool.alloc_buffer(conn, spec);
        }

        let &ShmAllocState::Uninit(wl_shm) = &self.state else {
            unreachable!()
        };

        self.state = ShmAllocState::Init(InitShmPool::new(conn, wl_shm, spec.size())?);
        if wl_shm.version() >= 2 {
            wl_shm.release(conn);
        }
        let ShmAllocState::Init(pool) = &mut self.state else {
            unreachable!()
        };
        pool.alloc_buffer(conn, spec)
    }

    /// Release all Wayland resources.
    pub fn destroy<D>(self, conn: &mut Connection<D>) {
        match self.state {
            ShmAllocState::Uninit(wl_shm) => {
                if wl_shm.version() >= 2 {
                    wl_shm.release(conn);
                }
            }
            ShmAllocState::Init(pool) => {
                pool.pool.destroy(conn);
            }
        }
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
            move |ctx| {
                assert!(refcnt.fetch_sub(1, Ordering::AcqRel) > 0);
                ctx.proxy.destroy(ctx.conn);
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

impl InitShmPool {
    fn new<D>(conn: &mut Connection<D>, wl_shm: WlShm, size: usize) -> io::Result<InitShmPool> {
        let file = shmemfdrs2::create_shmem(wayrs_client::cstr!("/wayrs_shm_pool"))?;
        file.set_len(size as u64)?;
        let mmap = unsafe { MmapMut::map_mut(&file)? };

        let fd_dup = file
            .as_fd()
            .try_clone_to_owned()
            .expect("could not duplicate fd");

        let pool = wl_shm.create_pool(conn, fd_dup, size as i32);

        Ok(Self {
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
        })
    }

    fn alloc_buffer<D>(
        &mut self,
        conn: &mut Connection<D>,
        spec: BufferSpec,
    ) -> io::Result<(Buffer, &mut [u8])> {
        let segment_index = self.alloc_segment(conn, spec)?;
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
                move |_| {
                    assert!(seg_refcnt.fetch_sub(1, Ordering::SeqCst) > 0);
                    // We don't destroy the buffer here because it can be reused later
                },
            );
            (wl, spec)
        });

        Ok((
            Buffer {
                spec,
                wl,
                refcnt: Arc::clone(&segment.refcnt),
                wl_shm_pool: self.pool,
                offset: segment.offset,
            },
            &mut self.mmap[segment.offset..][..segment.len],
        ))
    }

    fn defragment<D>(&mut self, conn: &mut Connection<D>) {
        let mut i = 0;
        while i + 1 < self.segments.len() {
            // `refcnt` cannot go from zero to anything else as it implies that the segment is not
            // used anymore.
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

    /// Resize the memmap, at least doubling the size.
    fn resize<D>(&mut self, conn: &mut Connection<D>, new_len: usize) -> io::Result<()> {
        if new_len > self.len {
            self.len = usize::max(self.len * 2, new_len);
            self.file.set_len(self.len as u64)?;
            self.pool.resize(conn, self.len as i32);
            self.mmap = unsafe { MmapMut::map_mut(&self.file)? };
        }
        Ok(())
    }

    /// Returns segment index, does not resize
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
        spec: BufferSpec,
    ) -> io::Result<usize> {
        let len = spec.size();

        if let Some(index) = self.try_alloc_in_place(conn, len, spec) {
            return Ok(index);
        }

        self.defragment(conn);
        if let Some(index) = self.try_alloc_in_place(conn, len, spec) {
            return Ok(index);
        }

        let segments_len = match self.segments.last_mut() {
            Some(segment)
                if segment
                    .refcnt
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok() =>
            {
                if let Some(buffer) = segment.buffer.take() {
                    buffer.0.destroy(conn);
                }
                segment.len = len;
                let new_size = segment.offset + segment.len;
                self.resize(conn, new_size)?;
                new_size
            }
            _ => {
                let offset = self.len;
                self.resize(conn, self.len + len)?;
                self.segments.push(Segment {
                    offset,
                    len,
                    refcnt: Arc::new(AtomicU32::new(1)),
                    buffer: None,
                });
                offset + len
            }
        };

        // Create a segment if `self.resize()` over allocated
        if segments_len > self.len {
            self.segments.push(Segment {
                offset: segments_len,
                len: self.len - segments_len,
                refcnt: Arc::new(AtomicU32::new(0)),
                buffer: None,
            });
        }

        Ok(self.segments.len() - 1)
    }
}
