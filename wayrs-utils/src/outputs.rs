//! wl_output helper

use std::ffi::CString;

use wayrs_client::connection::Connection;
use wayrs_client::global::*;
use wayrs_client::protocol::*;
use wayrs_client::proxy::Proxy;

pub trait OutputHandler: Sized + 'static {
    fn get_outputs(&mut self) -> &mut Outputs;

    /// Called when output is added and initial info is received.
    fn output_added(&mut self, _: &mut Connection<Self>, _: WlOutput) {}

    /// Called when output is removed.
    fn output_removed(&mut self, _: &mut Connection<Self>, _: WlOutput) {}

    /// Called when output info is updated after the initial info in sent.
    fn info_updated(&mut self, _: &mut Connection<Self>, _: WlOutput, _: UpdatesMask) {}
}

#[derive(Debug)]
pub struct Outputs {
    outputs: Vec<Output>,
}

#[derive(Debug)]
pub struct Output {
    pub reg_name: u32,
    pub wl_output: WlOutput,

    pub geometry: Option<wl_output::GeometryArgs>,
    pub mode: Option<wl_output::ModeArgs>,
    pub scale: u32,
    pub name: Option<CString>,
    pub description: Option<CString>,

    pending_update_mask: UpdatesMask,
    initial_info_received: bool,
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct UpdatesMask {
    pub geometry: bool,
    pub mode: bool,
    pub scale: bool,
    pub name: bool,
    pub description: bool,
}

impl Outputs {
    pub fn bind<D: OutputHandler>(conn: &mut Connection<D>, globals: &Globals) -> Self {
        conn.add_registry_cb(registry_cb);
        Self {
            outputs: globals
                .iter()
                .filter(|g| g.is::<WlOutput>())
                .map(|g| Output::bind(conn, g))
                .collect(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Output> + '_ {
        self.outputs.iter()
    }
}

impl Output {
    fn bind<D: OutputHandler>(conn: &mut Connection<D>, global: &Global) -> Self {
        Self {
            reg_name: global.name,
            wl_output: global.bind_with_cb(conn, 1..=4, wl_output_cb).unwrap(),

            geometry: None,
            mode: None,
            scale: 1,
            name: None,
            description: None,

            pending_update_mask: UpdatesMask::default(),
            initial_info_received: false,
        }
    }
}

fn registry_cb<D: OutputHandler>(
    conn: &mut Connection<D>,
    state: &mut D,
    event: &wl_registry::Event,
) {
    let output_state = state.get_outputs();

    match event {
        wl_registry::Event::Global(g) if g.is::<WlSeat>() => {
            let output = Output::bind(conn, g);
            let wl_output = output.wl_output;
            output_state.outputs.push(output);

            state.output_added(conn, wl_output);
        }
        wl_registry::Event::GlobalRemove(name) => {
            let Some(i) = output_state.outputs.iter().position(|o| o.reg_name == *name)
            else { return };

            let output = output_state.outputs.swap_remove(i);

            state.output_removed(conn, output.wl_output);

            if output.wl_output.version() >= 3 {
                output.wl_output.release(conn);
            }
        }
        _ => (),
    }
}

fn wl_output_cb<D: OutputHandler>(
    conn: &mut Connection<D>,
    state: &mut D,
    wl_output: WlOutput,
    event: wl_output::Event,
) {
    let output = state
        .get_outputs()
        .outputs
        .iter_mut()
        .find(|o| o.wl_output == wl_output)
        .unwrap();

    // "done" event is since version 2
    let mut is_done = wl_output.version() < 2;

    match event {
        wl_output::Event::Geometry(args) => {
            output.geometry = Some(args);
            output.pending_update_mask.geometry = true;
        }
        wl_output::Event::Mode(args) => {
            // non-current modes are deprecated
            if !args.flags.contains(wl_output::Mode::Current) {
                return;
            }

            output.mode = Some(args);
            output.pending_update_mask.mode = true;
        }
        wl_output::Event::Scale(scale) => {
            output.scale = scale.try_into().unwrap();
            output.pending_update_mask.scale = true;
        }
        wl_output::Event::Name(name) => {
            output.name = Some(name);
            output.pending_update_mask.name = true;
        }
        wl_output::Event::Description(desc) => {
            output.description = Some(desc);
            output.pending_update_mask.description = true;
        }
        wl_output::Event::Done => {
            is_done = true;
        }
    }

    if is_done {
        if output.initial_info_received {
            let mask = std::mem::take(&mut output.pending_update_mask);
            state.info_updated(conn, wl_output, mask);
        } else {
            output.initial_info_received = true;
            state.output_added(conn, wl_output);
        }
    }
}
