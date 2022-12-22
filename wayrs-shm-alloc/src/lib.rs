use std::fs::File;
use std::os::unix::io::{BorrowedFd, FromRawFd};

use memmap2::MmapMut;

use wayrs_client::event_queue::EventQueue;
use wayrs_client::protocol::{wl_buffer, wl_shm, wl_shm_pool};
use wayrs_client::proxy::{Dispatch, Dispatcher};

use wl_buffer::WlBuffer;
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
    pub fn new<D>(event_queue: &mut EventQueue<D>, wl_shm: WlShm, initial_len: usize) -> Self
    where
        D: Dispatch<WlShmPool>,
    {
        let fd = shmemfdrs::create_shmem(wayrs_client::cstr!("/ramp-buffer"), initial_len);
        let file = unsafe { File::from_raw_fd(fd) };
        let mmap = unsafe { memmap2::MmapMut::map_mut(&file).expect("memory mapping failed") };

        let fd_dup = unsafe {
            BorrowedFd::borrow_raw(fd)
                .try_clone_to_owned()
                .expect("could not duplicate fd")
        };
        let pool = wl_shm.create_pool(event_queue, fd_dup, initial_len as i32);

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

    pub fn alloc_buffer<D>(
        &mut self,
        event_queue: &mut EventQueue<D>,
        width: i32,
        height: i32,
        stride: i32,
        format: Format,
    ) -> (wl_buffer::WlBuffer, &mut [u8])
    where
        D: Dispatch<wl_shm_pool::WlShmPool> + Dispatch<wl_buffer::WlBuffer>,
    {
        let size = height * stride;

        let segment_index = self.alloc_segment(event_queue, size as usize, format);
        let segment = &mut self.segments[segment_index];

        let buffer = segment.buffer.get_or_insert_with(|| Buffer {
            wl: self.pool.create_buffer(
                event_queue,
                segment.offset as i32,
                width,
                height,
                stride,
                format,
            ),
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

    fn merge_segments<D: Dispatcher>(&mut self, event_queue: &mut EventQueue<D>) {
        let mut i = 0;
        while i + 1 < self.segments.len() {
            if self.segments[i].free && self.segments[i + 1].free {
                if let Some(buffer) = self.segments[i].buffer.take() {
                    buffer.wl.destroy(event_queue);
                }
                if let Some(buffer) = self.segments[i + 1].buffer.take() {
                    buffer.wl.destroy(event_queue);
                }
                self.segments[i].len += self.segments[i + 1].len;
                self.segments.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }

    fn resize<D: Dispatcher>(&mut self, event_queue: &mut EventQueue<D>, new_len: usize) {
        if new_len > self.len {
            self.len = new_len;
            self.file.set_len(new_len as u64).unwrap();
            self.pool.resize(event_queue, new_len as i32);
            self.mmap =
                unsafe { memmap2::MmapMut::map_mut(&self.file).expect("memory mapping failed") };
        }
    }

    // Returns segment index, does not resize
    fn try_alloc_in_place<D>(
        &mut self,
        event_queue: &mut EventQueue<D>,
        len: usize,
    ) -> Option<usize>
    where
        D: Dispatch<wl_shm_pool::WlShmPool> + Dispatch<wl_buffer::WlBuffer>,
    {
        // Find a segment with exact size
        for (i, segment) in self.segments.iter_mut().enumerate() {
            if segment.free && segment.len == len {
                if let Some(buffer) = segment.buffer.take() {
                    buffer.wl.destroy(event_queue);
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
                    buffer.wl.destroy(event_queue);
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
    fn alloc_segment<D>(
        &mut self,
        event_queue: &mut EventQueue<D>,
        len: usize,
        format: wl_shm::Format,
    ) -> usize
    where
        D: Dispatch<wl_shm_pool::WlShmPool> + Dispatch<wl_buffer::WlBuffer>,
    {
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

        if let Some(index) = self.try_alloc_in_place(event_queue, len) {
            return index;
        }
        self.merge_segments(event_queue);
        if let Some(index) = self.try_alloc_in_place(event_queue, len) {
            return index;
        }

        match self.segments.last_mut() {
            Some(segment) if segment.free => {
                if let Some(buffer) = segment.buffer.take() {
                    buffer.wl.destroy(event_queue);
                }
                let new_size = self.len + len - segment.len;
                segment.len = len;
                segment.free = false;
                self.resize(event_queue, new_size);
            }
            _ => {
                let offset = self.len;
                self.resize(event_queue, self.len + len);
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
