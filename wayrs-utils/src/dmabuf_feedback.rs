//! linux-dmabuf feedback event helper
//!
//! See <https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/unstable/linux-dmabuf/feedback.rst>
//! to learn what dmabuf feedback is and how it is used.
//!
//! To use this helper, implement [`DmabufFeedbackHandler`] for your state and create an
//! instance/instances of [`DmabufFeedback`]. When the feedback is received or updated, you will be
//! notified via [`DmabufFeedbackHandler::feedback_done`] callback.

use libc::dev_t;
use std::fmt;
use std::os::unix::net::UnixStream;

use wayrs_client::protocol::WlSurface;
use wayrs_client::{Connection, EventCtx};
use wayrs_protocols::linux_dmabuf_unstable_v1::*;

#[derive(Debug)]
pub struct DmabufFeedback {
    wl: ZwpLinuxDmabufFeedbackV1,
    main_device: Option<dev_t>,
    format_table: Option<memmap2::Mmap>,
    tranches: Vec<DmabufTranche>,
    pending_tranche: DmabufTranche,
    tranches_done: bool,
}

#[derive(Debug, Default)]
pub struct DmabufTranche {
    pub target_device: Option<dev_t>,
    pub formats: Option<Vec<u16>>,
    pub flags: zwp_linux_dmabuf_feedback_v1::TrancheFlags,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct FormatTableEntry {
    pub fourcc: u32,
    _padding: u32,
    pub modifier: u64,
}

pub trait DmabufFeedbackHandler<T = UnixStream>: Sized + 'static {
    /// Get a reference to a [`DmabufFeedback`] associated with `wl`.
    ///
    /// Returning a reference to a wrong object may cause [`Connection::dispatch_events`] to panic.
    fn get_dmabuf_feedback(&mut self, wl: ZwpLinuxDmabufFeedbackV1) -> &mut DmabufFeedback;

    /// A feedback for `wl` is received/updated.
    fn feedback_done(&mut self, conn: &mut Connection<Self, T>, wl: ZwpLinuxDmabufFeedbackV1);
}

impl DmabufFeedback {
    pub fn get_default<D: DmabufFeedbackHandler<T>, T: 'static>(
        conn: &mut Connection<D, T>,
        linux_dmabuf: ZwpLinuxDmabufV1,
    ) -> Self {
        Self {
            wl: linux_dmabuf.get_default_feedback_with_cb(conn, dmabuf_feedback_cb),
            main_device: None,
            format_table: None,
            tranches: Vec::new(),
            pending_tranche: DmabufTranche::default(),
            tranches_done: false,
        }
    }

    pub fn get_for_surface<D: DmabufFeedbackHandler<T>, T: 'static>(
        conn: &mut Connection<D, T>,
        linux_dmabuf: ZwpLinuxDmabufV1,
        surface: WlSurface,
    ) -> Self {
        Self {
            wl: linux_dmabuf.get_surface_feedback_with_cb(conn, surface, dmabuf_feedback_cb),
            main_device: None,
            format_table: None,
            tranches: Vec::new(),
            pending_tranche: DmabufTranche::default(),
            tranches_done: false,
        }
    }

    pub fn wl(&self) -> ZwpLinuxDmabufFeedbackV1 {
        self.wl
    }

    pub fn main_device(&self) -> Option<dev_t> {
        self.main_device
    }

    pub fn format_table(&self) -> &[FormatTableEntry] {
        match &self.format_table {
            Some(mmap) => unsafe {
                std::slice::from_raw_parts(
                    mmap.as_ptr().cast(),
                    mmap.len() / std::mem::size_of::<FormatTableEntry>(),
                )
            },
            None => &[],
        }
    }

    pub fn tranches(&self) -> &[DmabufTranche] {
        &self.tranches
    }

    pub fn destroy<D, T>(self, conn: &mut Connection<D, T>) {
        self.wl.destroy(conn);
    }
}

fn dmabuf_feedback_cb<D: DmabufFeedbackHandler<T>, T>(
    ctx: EventCtx<D, ZwpLinuxDmabufFeedbackV1, T>,
) {
    let feedback = ctx.state.get_dmabuf_feedback(ctx.proxy);
    assert_eq!(
        feedback.wl, ctx.proxy,
        "invalid DmabufFeedbackHandler::get_dmabuf_feedback() implementation"
    );

    use zwp_linux_dmabuf_feedback_v1::Event;
    match ctx.event {
        Event::Done => {
            feedback.tranches_done = true;
            ctx.state.feedback_done(ctx.conn, ctx.proxy);
        }
        Event::FormatTable(args) => {
            let mmap = unsafe {
                memmap2::MmapOptions::new()
                    .len(args.size as usize)
                    .map_copy_read_only(&args.fd)
                    .expect("mmap failed")
            };
            assert!(
                ptr_is_aligned(mmap.as_ptr().cast::<FormatTableEntry>()),
                "memory map is not alligned"
            );
            feedback.format_table = Some(mmap);
        }
        Event::MainDevice(main_dev) => {
            feedback.main_device = Some(dev_t::from_ne_bytes(
                main_dev.try_into().expect("invalid main_device size"),
            ));
        }
        Event::TrancheDone => {
            let tranche = std::mem::take(&mut feedback.pending_tranche);
            feedback.tranches.push(tranche);
        }
        Event::TrancheTargetDevice(target_dev) => {
            if feedback.tranches_done {
                feedback.tranches.clear();
                feedback.tranches_done = false;
            }
            feedback.pending_tranche.target_device = Some(dev_t::from_ne_bytes(
                target_dev
                    .try_into()
                    .expect("invalid tranche_target_device size"),
            ));
        }
        Event::TrancheFormats(indices) => {
            if feedback.tranches_done {
                feedback.tranches.clear();
                feedback.tranches_done = false;
            }
            // TODO: check alignment and do Vec::into_raw_parts + Vec::from_raw_parts to avoid unnecessary allocation
            let mut formats = Vec::with_capacity(indices.len() / 2);
            for index in indices.chunks_exact(2) {
                let index = u16::from_ne_bytes(index.try_into().unwrap());
                formats.push(index);
            }
            feedback.pending_tranche.formats = Some(formats);
        }
        Event::TrancheFlags(flags) => {
            if feedback.tranches_done {
                feedback.tranches.clear();
                feedback.tranches_done = false;
            }
            feedback.pending_tranche.flags = flags;
        }
        _ => (),
    }
}

impl fmt::Debug for FormatTableEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.fourcc.to_le_bytes();
        write!(
            f,
            "{}{}{}{}:{}",
            a.escape_ascii(),
            b.escape_ascii(),
            c.escape_ascii(),
            d.escape_ascii(),
            self.modifier
        )
    }
}

fn ptr_is_aligned<T>(ptr: *const T) -> bool {
    (ptr as usize) & (std::mem::align_of::<T>() - 1) == 0
}
