use std::convert::Infallible;
use std::ffi::CString;

use wayrs_client::connection::Connection;
use wayrs_client::global::GlobalExt;
use wayrs_client::protocol::wl_output::{self, WlOutput};
use wayrs_client::protocol::wl_registry::{self, GlobalArgs, WlRegistry};
use wayrs_client::proxy::{Dispatch, Dispatcher};
use wayrs_client::socket::IoMode;

fn main() {
    let mut conn = Connection::connect().unwrap();
    let mut state = State::default();

    loop {
        conn.flush(IoMode::Blocking).unwrap();
        conn.recv_events(IoMode::Blocking).unwrap();
        conn.dispatch_events(&mut state).unwrap();
    }
}

#[derive(Default)]
struct State {
    outputs: Vec<Output>,
}

#[derive(Debug)]
struct Output {
    registry_name: u32,
    wl_output: WlOutput,
    name: Option<CString>,
    desc: Option<CString>,
    scale: Option<i32>,
    mode: Option<String>,
}

impl Output {
    fn bind(conn: &mut Connection<State>, global: &GlobalArgs) -> Self {
        Self {
            registry_name: global.name,
            wl_output: global.bind(conn, 3..=4).unwrap(),
            name: None,
            desc: None,
            scale: None,
            mode: None,
        }
    }
}

impl Dispatcher for State {
    type Error = Infallible;
}

impl Dispatch<WlRegistry> for State {
    fn event(&mut self, conn: &mut Connection<Self>, _: WlRegistry, event: wl_registry::Event) {
        match event {
            wl_registry::Event::Global(global) if global.is::<WlOutput>() => {
                self.outputs.push(Output::bind(conn, &global));
            }
            wl_registry::Event::GlobalRemove(name) => {
                if let Some(i) = self.outputs.iter().position(|o| o.registry_name == name) {
                    let output = self.outputs.swap_remove(i);
                    eprintln!("removed output: {}", output.name.unwrap().to_str().unwrap());
                    output.wl_output.release(conn);
                }
            }
            _ => (),
        }
    }
}

impl Dispatch<WlOutput> for State {
    fn event(&mut self, _: &mut Connection<Self>, output: WlOutput, event: wl_output::Event) {
        let output = &mut self
            .outputs
            .iter_mut()
            .find(|o| o.wl_output == output)
            .unwrap();
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
            wl_output::Event::Done => {
                dbg!(output);
            }
            wl_output::Event::Scale(scale) => output.scale = Some(scale),
            wl_output::Event::Name(name) => output.name = Some(name),
            wl_output::Event::Description(desc) => output.desc = Some(desc),
        }
    }
}
