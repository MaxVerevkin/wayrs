//! wl_seat helper

use std::ffi::CString;

use wayrs_client::connection::Connection;
use wayrs_client::global::*;
use wayrs_client::protocol::wl_seat::Capability;
use wayrs_client::protocol::*;
use wayrs_client::proxy::Proxy;

pub trait SeatHandler: Sized + 'static {
    fn get_seats(&mut self) -> &mut Seats;

    /// A new seat is added.
    ///
    /// This is _not_ called for seats that where advertised during the initial roundtrip.
    fn seat_added(&mut self, _: &mut Connection<Self>, _: WlSeat) {}

    /// A seat is removed.
    fn seat_removed(&mut self, _: &mut Connection<Self>, _: WlSeat) {}

    /// Seat name was advertised.
    fn seat_name(&mut self, _: &mut Connection<Self>, _: WlSeat, _name: CString) {}

    /// Pointer capability was added.
    fn pointer_added(&mut self, _: &mut Connection<Self>, _: WlSeat) {}

    /// Pointer capability or seat was removed
    fn pointer_removed(&mut self, _: &mut Connection<Self>, _: WlSeat) {}

    /// Keyboard capability was added.
    fn keyboard_added(&mut self, _: &mut Connection<Self>, _: WlSeat) {}

    /// Keyboard capability or seat was removed
    fn keyboard_removed(&mut self, _: &mut Connection<Self>, _: WlSeat) {}

    /// Touch capability was added.
    fn touch_added(&mut self, _: &mut Connection<Self>, _: WlSeat) {}

    /// Touch capability or seat was removed
    fn touch_removed(&mut self, _: &mut Connection<Self>, _: WlSeat) {}
}

/// The state of `wl_seat`s.
///
/// This struct keeps track of currently available `wl_seat`s and their capabilities.
#[derive(Debug)]
pub struct Seats {
    seats: Vec<Seat>,
}

#[derive(Debug)]
struct Seat {
    reg_name: u32,
    wl_seat: WlSeat,
    capabilities: Capability,
}

impl Seats {
    pub fn bind<D: SeatHandler>(conn: &mut Connection<D>, globals: &Globals) -> Self {
        conn.add_registry_cb(registry_cb);
        Self {
            seats: globals
                .iter()
                .filter(|g| g.is::<WlSeat>())
                .map(|g| Seat::bind(conn, g))
                .collect(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = WlSeat> + '_ {
        self.seats.iter().map(|s| s.wl_seat)
    }
}

impl Seat {
    fn bind<D: SeatHandler>(conn: &mut Connection<D>, global: &Global) -> Self {
        Self {
            reg_name: global.name,
            wl_seat: global.bind_with_cb(conn, 1..=8, wl_seat_cb).unwrap(),
            capabilities: Capability::empty(),
        }
    }
}

fn registry_cb<D: SeatHandler>(
    conn: &mut Connection<D>,
    state: &mut D,
    event: &wl_registry::Event,
) {
    let seat_state = state.get_seats();

    match event {
        wl_registry::Event::Global(g) if g.is::<WlSeat>() => {
            let seat = Seat::bind(conn, g);
            let wl_seat = seat.wl_seat;
            seat_state.seats.push(seat);

            state.seat_added(conn, wl_seat);
        }
        wl_registry::Event::GlobalRemove(name) => {
            let Some(i) = seat_state.seats.iter().position(|s| s.reg_name == *name) else { return };
            let seat = seat_state.seats.swap_remove(i);

            if seat.capabilities.contains(Capability::Pointer) {
                state.pointer_removed(conn, seat.wl_seat);
            }
            if seat.capabilities.contains(Capability::Keyboard) {
                state.keyboard_removed(conn, seat.wl_seat);
            }
            if seat.capabilities.contains(Capability::Touch) {
                state.touch_removed(conn, seat.wl_seat);
            }

            state.seat_removed(conn, seat.wl_seat);

            if seat.wl_seat.version() >= 5 {
                seat.wl_seat.release(conn);
            }
        }
        _ => (),
    }
}

fn wl_seat_cb<D: SeatHandler>(
    conn: &mut Connection<D>,
    state: &mut D,
    wl_seat: WlSeat,
    event: wl_seat::Event,
) {
    let seat = state
        .get_seats()
        .seats
        .iter_mut()
        .find(|s| s.wl_seat == wl_seat)
        .unwrap();

    match event {
        wl_seat::Event::Capabilities(new_caps) => {
            let old_caps = seat.capabilities;
            seat.capabilities = new_caps;

            match (
                new_caps.contains(Capability::Pointer),
                old_caps.contains(Capability::Pointer),
            ) {
                (true, false) => state.pointer_added(conn, wl_seat),
                (false, true) => state.pointer_removed(conn, wl_seat),
                _ => (),
            }

            match (
                new_caps.contains(Capability::Keyboard),
                old_caps.contains(Capability::Keyboard),
            ) {
                (true, false) => state.keyboard_added(conn, wl_seat),
                (false, true) => state.keyboard_removed(conn, wl_seat),
                _ => (),
            }

            match (
                new_caps.contains(Capability::Touch),
                old_caps.contains(Capability::Touch),
            ) {
                (true, false) => state.touch_added(conn, wl_seat),
                (false, true) => state.touch_removed(conn, wl_seat),
                _ => (),
            }
        }
        wl_seat::Event::Name(name) => {
            state.seat_name(conn, wl_seat, name);
        }
        _ => (),
    }
}
