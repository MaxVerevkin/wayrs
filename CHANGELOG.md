## 0.9.0

- Generate documentation for protocol enums.
- Implement `Hash` for enums.
- Remove `From<u32>` implementation for non-bitfield enums.
- Mark enums as `non_exhaustive`.

## 0.8.0

- `Proxy` trait: remove `fn interface() -> &Interface` in favor of associated `const INTERFACE`.
-  Prohibit calling `Connection::add_registry_cb` and `Connection::dispatch_events` from callback context.

## 0.7.0

- Support NULL-able strings in arguments and events.
- Added `Proxy::is_null()` method.

## 0.6.0

- Mark events as `non_exhaustive`.
- Update protocols.

## 0.5.0

- Make `Connection` `Send` by requiring registry callbacks to be `Send`.

## 0.4.0

- `wl_registry` can now have any number of callbacks.
- `Connection::set_callback_for` now panics if called for `wl_registry`. Use `Connection::add_registry_cb` instead.
- Introduce `wayrs_utils`: a collection of small and modular utils and abstractions.
- `wayrs_shm_alloc` and `wayrs_cursor` were moved to `wayrs_utils`.

## 0.3.0

- Add debug messages (set `WAYRS_DEBUG=1` env variable to enable).
- Drop `Dispatch` trait machinery in favor of per-object callbacks. Makes it easier to write libraries.
- Rename `socket::IoMode` to `IoMode`.
- Make `socket` module private.
- Reexport `wayrs_scanner` as `wayrs_client::scanner`.
