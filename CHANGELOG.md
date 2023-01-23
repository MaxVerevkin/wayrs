## 0.3.0

- Add debug messages (set `WAYRS_DEBUG=1` env variable to enable).
- Drop `Dispatch` trait machinery in favor of per-object callbacks. Makes it easier to write libraries.
- Rename `socket::IoMode` to `IoMode`.
- Make `socket` module private.
- Reexport `wayrs_scanner` as `wayrs_client::scanner`.
