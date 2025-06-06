# 1.3.1

- Deprecate `Client::clear_callbacks`.

# 1.3.0

- Bump MSRV to 1.79.
- Deprecate `cstr!` macro.
- Mark some functions as `#[must_use]`.
- Support `WAYLAND_SOCKET` environment variable.

# 1.2.0

- Store a list of globals in the `Connection` struct.
- Deprecate `Connection::connect_and_collect_globals`, `Connection::async_connect_and_collect_globals`, `GlobalsExt::bind` and `GlobalsExt::bind_with_cb`. Instead use `Connection::blocking_roundtrip`/`Connection::async_roundtrip` followed with `Connection::bind_singleton`/`Connection::bind_singleton_with_cb`.
- Derive `Clone` and `Copy` on event structs when possible.

# 1.1.3

- Drop `thiserror` dependency.

# 1.1.2

- Drop `proc-macro-crate` transitive dependency by updating `wayrs-scanner`.

# 1.1.1

- Include license file in the package.

# 1.1.0

- Refactor the core functionality and types into `wayrs-core`, which can be used by both clinets and servers.
- Drop `nix` dependency.
- Reduce memory allocations by reusing buffers between messages.
- The `Proxy` trait was changed to make the optimization above possible, but the intended usage should work the same.
- The `interface`, `proxy` and `wire` modules are deprecated and were made doc-hidden. `interface::*`/`wire::*` is now in `core::*` and `proxy::*` is now in `object::*`.

# 1.0.3

- Update `nix` to v0.28.

# 1.0.2

- Update for `wayrs-scanner` v0.13.

# 1.0.1

- Reduce memory allocations.
- Update core protocol.

# 1.0.0

- Drog `..` and `a..` support from `Global::bind`. Provide the upper bound.
- Rename `wayrs_client::scanner::generate!` to `wayrs_client::generate!`.
- Impl From<{f32,f64}> for `Fixed`.

# 0.12.4

- Fixed typo in `Connection::clear_callbacks()` (`clear_callbacs` is deprecated).

# 0.12.3

- Fix binding of globals with no upper version limit.
- Add `Connection::clear_callbacks()`.

# 0.12.2

- Prevent excessive socket flushes.
- Use ring buffers for in/out bytes. For reference, ring buffers are also used by `wayland-client`.

# 0.12.1

- Proxies and `Object` can now be compared with `ObjectId`.
- Implement `Borrow<ObjectId>` for proxies and `Object`.
- MSRV is now 1.66.

# 0.12.0

- Merge callback args into `EventCtx` struct.
- Update `nix` to v0.27.

Migration example:

```rust
// Before
fn wl_output_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    output: WlOutput,
    event: wl_output::Event,
) {
    todo!();
}

// After
fn wl_output_cb(ctx: EventCtx<State, WlOutput>) {
    todo!();
}
```

# 0.11.1

- Improve `Debug` implementation for `Fixed`.
- Add `Fixed::as_int()` and `Fixed::as_f32()`.
- Store the socket buffer on the heap (significantly reduces `Connection` stack size).

# 0.11.0

- Update core protocol to [72da004b](72da004b3eed19a94265d564f1fa59276ceb4340).
- Make `ObjectId` opaque, make associated constants private and remove deprecated `ObjectId::next`.
- Implement `Ord` for `Object`.
- Make `wl_display` private.
- Rename `connection::Connection` to `Connection`;
- Use `WAYLAND_DEBUG` env var instead of custom `WAYRS_DEBUG`.
- Remove `Connection::blocking_collect_initial_globals` and `Connection::async_collect_initial_globals` in favour of `Connection::connect_and_collect_globals` and `Connection::async_connect_and_collect_globals`.
- Do not print "no callback for ..." debug messages.
- Improve debug printing.
- `Global::bind` and `Global::bind_with_cb` now accpet version as a number, a full range (`..`), a range from (`a..`), a range to (`..=b`) and a range (`a..=b`).
