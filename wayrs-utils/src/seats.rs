//! wl_seat helper
//!
//! To use this abstraction, create an instance of [`Seats`] using [`Seats::bind`], store it in your
//! state struct and finally implement [`SeatHandler`] for your state type.
//!
//! # Example
//!
//! ```no_run
//! use wayrs_utils::seats::*;
//! use wayrs_client::Connection;
//! use wayrs_client::protocol::*;
//! use wayrs_client::object::Proxy;
//!
//! #[derive(Debug)]
//! struct State {
//!     seats: Seats,
//!     keyboards: Vec<(WlSeat, WlKeyboard)>,
//! }
//!
//! impl SeatHandler for State {
//!     fn get_seats(&mut self) -> &mut Seats {
//!         &mut self.seats
//!     }
//!
//!     // All other functions are optional to implement
//!
//!     fn keyboard_added(&mut self, conn: &mut Connection<Self>, seat: WlSeat) {
//!         self.keyboards.push((seat, seat.get_keyboard(conn)));
//!     }
//!
//!     fn keyboard_removed(&mut self, conn: &mut Connection<Self>, seat: WlSeat) {
//!         let i = self.keyboards.iter().position(|&(s, _)| s == seat).unwrap();
//!         let (_, keyboard) = self.keyboards.swap_remove(i);
//!         if keyboard.version() >= 3 {
//!             keyboard.release(conn);
//!         }
//!     }
//! }
//!
//! let mut conn = Connection::connect().unwrap();
//!
//! let mut state = State {
//!     seats: Seats::bind(&mut conn),
//!     keyboards: Vec::new(),
//! };
//!
//! conn.blocking_roundtrip().unwrap();
//! conn.dispatch_events(&mut state);
//!
//! dbg!(state);
//! ```

use std::ffi::CString;

use wayrs_client::global::*;
use wayrs_client::object::Proxy;
use wayrs_client::protocol::wl_seat::Capability;
use wayrs_client::protocol::*;
use wayrs_client::{Connection, EventCtx};

pub trait SeatHandler: Sized + 'static {
    fn get_seats(&mut self) -> &mut Seats;

    /// A new seat is added.
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
    /// Create new `Seats`.
    ///
    /// This function sets up the registry callback and nothing else. Call it only once per
    /// [`Connection`](Connection) and before dispatching any events.
    pub fn new<D: SeatHandler>(conn: &mut Connection<D>) -> Self {
        conn.add_registry_cb(registry_cb);
        Self { seats: Vec::new() }
    }

    #[deprecated = "use `new` instead (this name is misleading, it does not bind anything)"]
    pub fn bind<D: SeatHandler>(conn: &mut Connection<D>) -> Self {
        Self::new(conn)
    }

    /// Get an iterator of currently available `wl_seat`s.
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
            let Some(i) = seat_state.seats.iter().position(|s| s.reg_name == *name) else {
                return;
            };
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

fn wl_seat_cb<D: SeatHandler>(ctx: EventCtx<D, WlSeat>) {
    let seat = ctx
        .state
        .get_seats()
        .seats
        .iter_mut()
        .find(|s| s.wl_seat == ctx.proxy)
        .unwrap();

    match ctx.event {
        wl_seat::Event::Capabilities(new_caps) => {
            let old_caps = seat.capabilities;
            seat.capabilities = new_caps;

            match (
                new_caps.contains(Capability::Pointer),
                old_caps.contains(Capability::Pointer),
            ) {
                (true, false) => ctx.state.pointer_added(ctx.conn, ctx.proxy),
                (false, true) => ctx.state.pointer_removed(ctx.conn, ctx.proxy),
                _ => (),
            }

            match (
                new_caps.contains(Capability::Keyboard),
                old_caps.contains(Capability::Keyboard),
            ) {
                (true, false) => ctx.state.keyboard_added(ctx.conn, ctx.proxy),
                (false, true) => ctx.state.keyboard_removed(ctx.conn, ctx.proxy),
                _ => (),
            }

            match (
                new_caps.contains(Capability::Touch),
                old_caps.contains(Capability::Touch),
            ) {
                (true, false) => ctx.state.touch_added(ctx.conn, ctx.proxy),
                (false, true) => ctx.state.touch_removed(ctx.conn, ctx.proxy),
                _ => (),
            }
        }
        wl_seat::Event::Name(name) => {
            ctx.state.seat_name(ctx.conn, ctx.proxy, name);
        }
        _ => (),
    }
}
