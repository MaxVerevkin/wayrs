use wayrs_client::Connection;

fn main() {
    let (_conn, initial_globals) = Connection::<()>::connect_and_collect_globals().unwrap();

    for global in initial_globals {
        println!(
            "{} v{}",
            global.interface.into_string().unwrap(),
            global.version
        );
    }
}
