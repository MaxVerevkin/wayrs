use wayrs_client::Connection;

fn main() {
    let mut conn = Connection::<()>::connect().unwrap();
    let initial_globals = conn.blocking_collect_initial_globals().unwrap();

    for global in initial_globals {
        println!(
            "{} v{}",
            global.interface.into_string().unwrap(),
            global.version
        );
    }
}
