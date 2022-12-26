use std::convert::Infallible;

use wayrs_client::connection::Connection;
use wayrs_client::protocol::wl_registry::WlRegistry;
use wayrs_client::proxy::{Dispatch, Dispatcher};

fn main() {
    let mut conn = Connection::<S>::connect().unwrap();
    let initial_globals = conn.blocking_collect_initial_globals().unwrap();

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
