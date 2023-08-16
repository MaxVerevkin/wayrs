# Next

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
