use wayrs_client::Connection;

fn main() {
    let mut conn = Connection::<()>::connect().unwrap();
    conn.blocking_roundtrip().unwrap();

    for global in conn.globals() {
        println!("{} v{}", global.interface.to_string_lossy(), global.version);
    }
}
