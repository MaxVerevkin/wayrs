use std::ffi::CString;

use wayrs_client::global::GlobalExt;
use wayrs_client::protocol::wl_output::{self, WlOutput};
use wayrs_client::{Connection, EventCtx, IoMode};

fn main() {
    let (mut conn, globals) = Connection::connect_and_collect_globals().unwrap();

    let mut state = State {
        outputs: globals
            .iter()
            .filter(|g| g.is::<WlOutput>())
            .map(|g| g.bind_with_cb(&mut conn, 2..=4, wl_output_cb).unwrap())
            .map(|output| (output, OutputInfo::default()))
            .collect(),
    };

    conn.flush(IoMode::Blocking).unwrap();

    while !state.outputs.iter().all(|x| x.1.done) {
        conn.recv_events(IoMode::Blocking).unwrap();
        conn.dispatch_events(&mut state);
    }

    for (_, output) in state.outputs {
        dbg!(output);
    }
}

struct State {
    outputs: Vec<(WlOutput, OutputInfo)>,
}

#[derive(Debug, Default)]
struct OutputInfo {
    done: bool,
    name: Option<CString>,
    desc: Option<CString>,
    scale: Option<i32>,
    mode: Option<String>,
}

fn wl_output_cb(ctx: EventCtx<State, WlOutput>) {
    let output = &mut ctx
        .state
        .outputs
        .iter_mut()
        .find(|o| o.0 == ctx.proxy)
        .unwrap()
        .1;
    match ctx.event {
        wl_output::Event::Geometry(_) => (),
        wl_output::Event::Mode(mode) => {
            output.mode = Some(format!(
                "{}x{} @ {}Hz",
                mode.width,
                mode.height,
                mode.refresh as f64 * 1e-3
            ))
        }
        wl_output::Event::Done => output.done = true,
        wl_output::Event::Scale(scale) => output.scale = Some(scale),
        wl_output::Event::Name(name) => output.name = Some(name),
        wl_output::Event::Description(desc) => output.desc = Some(desc),
        _ => (),
    }
}
