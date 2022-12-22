use std::convert::Infallible;
use std::ffi::CString;

use wayrs_client::event_queue::EventQueue;
use wayrs_client::global::GlobalExt;
use wayrs_client::protocol::wl_output::{self, WlOutput};
use wayrs_client::protocol::wl_registry::WlRegistry;
use wayrs_client::proxy::{Dispatch, Dispatcher, Proxy};
use wayrs_client::socket::IoMode;

fn main() {
    let (initial_globals, mut event_queue) = EventQueue::blocking_init().unwrap();

    let mut state = State {
        outputs: Vec::new(),
    };

    for output in initial_globals
        .iter()
        .filter(|g| g.is::<WlOutput>())
        .map(|g| g.bind(&mut event_queue, 2..=4).unwrap())
    {
        state.outputs.push((output, OutputInfo::default()));
    }

    event_queue.connection().flush(IoMode::Blocking).unwrap();

    while !state.outputs.iter().all(|x| x.1.done) {
        event_queue.recv_events(IoMode::Blocking).unwrap();
        event_queue.dispatch_events(&mut state).unwrap();
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

impl Dispatcher for State {
    type Error = Infallible;
}

impl Dispatch<WlRegistry> for State {}

impl Dispatch<WlOutput> for State {
    fn event(&mut self, _: &mut EventQueue<Self>, output: WlOutput, event: wl_output::Event) {
        let output = &mut self
            .outputs
            .iter_mut()
            .find(|o| o.0.id() == output.id())
            .unwrap()
            .1;
        match event {
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
        }
    }
}
