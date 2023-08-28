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

use wayrs_client::protocol::WlSurface;
use wayrs_client::{Connection, EventCtx};
use wayrs_protocols::linux_dmabuf_unstable_v1::*;

#[derive(Debug)]
pub struct DmabufFeedback {
    pub main_device: Option<dev_t>,
    pub format_table: Vec<FormatTableEntry>,
    pub tranches: Vec<DmabufTranche>,

    wl: ZwpLinuxDmabufFeedbackV1,
    tranches_done: bool,
    pending_tranche: DmabufTranche,
}

#[derive(Debug, Default)]
pub struct DmabufTranche {
    pub target_device: Option<dev_t>,
    pub formats: Option<Vec<u16>>,
    pub flags: zwp_linux_dmabuf_feedback_v1::TrancheFlags,
}

#[derive(Clone, Copy)]
pub struct FormatTableEntry {
    pub fourcc: u32,
    pub modifier: u64,
}

pub trait DmabufFeedbackHandler: Sized + 'static {
    /// Get a reference to a [`DmabufFeedback`] associated with `wl`.
    ///
    /// Returning a reference to a wrong object may cause [`Connection::dispatch_events`] to panic.
    fn get_dmabuf_feedback(&mut self, wl: ZwpLinuxDmabufFeedbackV1) -> &mut DmabufFeedback;

    /// A feedback for `wl` is received/updated.
    fn feedback_done(&mut self, conn: &mut Connection<Self>, wl: ZwpLinuxDmabufFeedbackV1);
}

impl DmabufFeedback {
    pub fn get_default<D: DmabufFeedbackHandler>(
        conn: &mut Connection<D>,
        linux_dmabuf: ZwpLinuxDmabufV1,
    ) -> Self {
        Self {
            wl: linux_dmabuf.get_default_feedback_with_cb(conn, dmabuf_feedback_cb),
            main_device: None,
            format_table: Vec::new(),
            tranches: Vec::new(),

            tranches_done: false,
            pending_tranche: DmabufTranche::default(),
        }
    }

    pub fn get_for_surface<D: DmabufFeedbackHandler>(
        conn: &mut Connection<D>,
        linux_dmabuf: ZwpLinuxDmabufV1,
        surface: WlSurface,
    ) -> Self {
        Self {
            wl: linux_dmabuf.get_surface_feedback_with_cb(conn, surface, dmabuf_feedback_cb),
            main_device: None,
            format_table: Vec::new(),
            tranches: Vec::new(),

            tranches_done: false,
            pending_tranche: DmabufTranche::default(),
        }
    }

    pub fn wl(&self) -> ZwpLinuxDmabufFeedbackV1 {
        self.wl
    }

    pub fn destroy<D>(self, conn: &mut Connection<D>) {
        self.wl.destroy(conn);
    }
}

fn dmabuf_feedback_cb<D: DmabufFeedbackHandler>(ctx: EventCtx<D, ZwpLinuxDmabufFeedbackV1>) {
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
            feedback.format_table.clear();
            feedback.format_table.reserve((args.size / 16) as usize);
            let mmap = unsafe {
                memmap2::MmapOptions::new()
                    .len(args.size as usize)
                    .map(&args.fd)
                    .expect("mmap failed")
            };
            for pair in mmap.chunks_exact(16) {
                feedback.format_table.push(FormatTableEntry {
                    fourcc: u32::from_ne_bytes(pair[0..4].try_into().unwrap()),
                    modifier: u64::from_ne_bytes(pair[8..16].try_into().unwrap()),
                });
            }
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
