//! wl_keyboard helper

use std::fmt::{self, Debug};
use std::os::unix::io::AsRawFd;
use std::time::Duration;

use wayrs_client::protocol::wl_keyboard::{EnterArgs, LeaveArgs};
use wayrs_client::proxy::Proxy;
use wayrs_client::Connection;
use wayrs_client::{protocol::*, EventCtx};

pub use xkbcommon::xkb;

use crate::timer::Timer;

pub trait KeyboardHandler: Sized + 'static {
    /// Get a reference to a [`Keyboard`]. It is guaranteed that the requested keyboard was created
    /// in [`Keyboard::new`].
    fn get_keyboard(&mut self, wl_keyboard: WlKeyboard) -> &mut Keyboard;

    fn key_presed(&mut self, conn: &mut Connection<Self>, event: KeyboardEvent);

    fn key_released(&mut self, conn: &mut Connection<Self>, event: KeyboardEvent);

    fn enter_surface(&mut self, _: &mut Connection<Self>, _: WlKeyboard, _: EnterArgs) {}

    fn leave_surface(&mut self, _: &mut Connection<Self>, _: WlKeyboard, _: LeaveArgs) {}
}

/// A wrapper of `wl_keyboard`.
///
/// Manages `xkb::Context` and `xkb::State`.
pub struct Keyboard {
    seat: WlSeat,
    wl: WlKeyboard,
    xkb_context: xkb::Context,
    xkb_state: Option<xkb::State>,
    repeat_info: Option<RepeatInfo>,
}

#[derive(Debug, Clone, Copy)]
pub struct RepeatInfo {
    pub delay: Duration,
    pub interval: Duration,
}

#[derive(Clone)]
pub struct KeyboardEvent {
    pub seat: WlSeat,
    pub keyboard: WlKeyboard,
    pub serial: u32,
    pub time: u32,
    pub keycode: xkb::Keycode,
    /// How this key should be repeated.
    ///
    /// Present if the compositor advertised repeat info AND this key should be repeated (as defined
    /// by the current keymap).
    pub repeat_info: Option<RepeatInfo>,
    pub xkb_state: xkb::State,
}

impl Keyboard {
    /// Create a new `Keyboard`.
    ///
    /// Call this only when `wl_seat` advertises a keyboard capability.
    #[inline]
    pub fn new<D: KeyboardHandler>(conn: &mut Connection<D>, seat: WlSeat) -> Self {
        Self {
            seat,
            wl: seat.get_keyboard_with_cb(conn, wl_keyboard_cb),
            xkb_context: xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
            xkb_state: None,
            repeat_info: None,
        }
    }

    #[inline]
    pub fn seat(&self) -> WlSeat {
        self.seat
    }

    #[inline]
    pub fn wl_keyboard(&self) -> WlKeyboard {
        self.wl
    }

    #[inline]
    pub fn destroy<D>(self, conn: &mut Connection<D>) {
        if self.wl.version() >= 3 {
            self.wl.release(conn);
        }
    }
}

impl RepeatInfo {
    /// Create a timer.
    pub fn timer(self) -> Timer {
        Timer::new(self.delay, self.interval)
    }
}

fn wl_keyboard_cb<D: KeyboardHandler>(ctx: EventCtx<D, WlKeyboard>) {
    let kbd = ctx.state.get_keyboard(ctx.proxy);

    match ctx.event {
        wl_keyboard::Event::Keymap(args) if args.format == wl_keyboard::KeymapFormat::XkbV1 => {
            let keymap = unsafe {
                xkb::Keymap::new_from_fd(
                    &kbd.xkb_context,
                    args.fd.as_raw_fd(),
                    args.size as usize,
                    xkb::FORMAT_TEXT_V1,
                    xkb::KEYMAP_COMPILE_NO_FLAGS,
                )
            };
            if let Ok(Some(keymap)) = keymap {
                kbd.xkb_state = Some(xkb::State::new(&keymap));
            }
        }
        wl_keyboard::Event::Enter(args) => {
            ctx.state.enter_surface(ctx.conn, ctx.proxy, args);
        }
        wl_keyboard::Event::Leave(args) => {
            ctx.state.leave_surface(ctx.conn, ctx.proxy, args);
        }
        wl_keyboard::Event::Key(args) => {
            let Some(xkb_state) = kbd.xkb_state.clone() else {
                return;
            };

            let keycode = xkb::Keycode::new(args.key + 8);

            let repeat_info = if xkb_state.get_keymap().key_repeats(keycode) {
                kbd.repeat_info
            } else {
                None
            };

            let event = KeyboardEvent {
                seat: kbd.seat,
                keyboard: kbd.wl,
                serial: args.serial,
                time: args.time,
                keycode,
                repeat_info,
                xkb_state,
            };

            match args.state {
                wl_keyboard::KeyState::Released => ctx.state.key_released(ctx.conn, event),
                wl_keyboard::KeyState::Pressed => ctx.state.key_presed(ctx.conn, event),
                _ => (),
            }
        }
        wl_keyboard::Event::Modifiers(args) => {
            if let Some(xkb_state) = &mut kbd.xkb_state {
                xkb_state.update_mask(
                    args.mods_depressed,
                    args.mods_latched,
                    args.mods_locked,
                    0,
                    0,
                    args.group,
                );
            }
        }
        wl_keyboard::Event::RepeatInfo(args) => {
            if args.rate == 0 {
                kbd.repeat_info = None;
            } else if args.rate > 0 && args.delay > 0 {
                kbd.repeat_info = Some(RepeatInfo {
                    delay: Duration::from_millis(args.delay as u64),
                    interval: Duration::from_micros(1_000_000 / args.rate as u64),
                });
            }
        }
        _ => (),
    }
}

impl Debug for KeyboardEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyboardEvent")
            .field("seat", &self.seat)
            .field("keyboard", &self.keyboard)
            .field("serial", &self.serial)
            .field("time", &self.time)
            .field("keycode", &self.keycode)
            .field("repeat_info", &self.repeat_info)
            .field("xkb_state", &"???")
            .finish()
    }
}
