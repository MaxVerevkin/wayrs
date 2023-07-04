# wayrs

A simple Rust implementation of Wayland client library.

## Design decisions

- Single event queue
- No interior mutability
- No `libwayland` compatibility
- Support blocking, non-blocking and async IO

## Project structure

The project is split into multiple crates:

- `wayrs-client`: The main crate which implements Wayland wire protocol. Provides `Connection` type which represents open Wayland socket.
- `wayrs-scanner`: Provides `generate!` macro that generates glue code from `.xml` files. Generated code for the core protocol is already included in `wayrs-client`. Reexported as `wayrs_client::scanner`.
- `wayrs-protocols`: A collection of Wayland protocols to use with `wayrs-client`.
- `wayrs-utils`: A collection of utils and abstractions for `wayrs-client`. Includes a shared memory allocator and more.

## Projects using `wayrs`

The following projects use `wayrs` and may serve as additional usage examples:

- [`i3bar-river`]: Port of i3bar for river.
- [`i3status-rs`]: Feature-rich and resource-friendly replacement for i3status.
- [`river-kbd-layout-watcher`]: Prints current keyboard layout whenever it changes.
- [`wayidle`]: Waits until the compositor reports being N seconds idle.
- [`wl-gammarelay-rs`]: Provides DBus interface to control display temperature and brightness without flickering.
- [`wlr-which-key`]: Keymap manager for wlroots-based compositors.

[`i3bar-river`]: https://github.com/MaxVerevkin/i3bar-river
[`i3status-rs`]: https://github.com/greshake/i3status-rust/
[`river-kbd-layout-watcher`]: https://github.com/MaxVerevkin/river-kbd-layout-watcher
[`wayidle`]: https://git.sr.ht/~whynothugo/wayidle
[`wl-gammarelay-rs`]: https://github.com/MaxVerevkin/wl-gammarelay-rs
[`wlr-which-key`]: https://github.com/MaxVerevkin/wlr-which-key

## MSRV

1.65
