use std::convert::Infallible;

use wayrs_client::event_queue::EventQueue;
use wayrs_client::protocol::wl_registry::WlRegistry;
use wayrs_client::proxy::{Dispatch, Dispatcher};

fn main() {
    let (initial_globals, _event_queue) = EventQueue::<S>::blocking_init().unwrap();

    for global in initial_globals {
        println!(
            "{} v{}",
            global.interface.into_string().unwrap(),
            global.version
        );
    }
}

struct S;
impl Dispatcher for S {
    type Error = Infallible;
}
impl Dispatch<WlRegistry> for S {}
