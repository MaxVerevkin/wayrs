# 0.13.0

- Update `proc-macro-crate` dependency.
- Delegate protocol parsing to `wayrs-proto-parser`.
- Generate code which uses new API available in `wayrs-client` v1.0.1.

# 0.12.7

- Update docs.

# 0.12.6

- Do not use `concat!()` in generated docs because rust-analyzer doesn't support it.

# 0.12.5

- Do not mark `wl_collback::Event` and `wl_buffer::Event` as non-exhaustive. These two interfaces
  are documented as "frozen", so they will never introduce new events/requests.

# 0.12.4

- Update `quick-xml` dependency.

# 0.12.3

- Update `proc-macro-crate` dependency.

# 0.12.2

- Generate "See `Event` for the list of possible events" in docs.

# 0.12.1

- Generate docs for event arguments.

# 0.12.0

- Update for `wayrs-client` v0.12.

# 0.11.1

- Use a `where` clause for `...with_cb` requests (for docs readability).
- Update `quick-xml` to v0.30.

# 0.11.0

- Delegate `Eq` and `Hash` proxy implementations to their IDs.
- Implement `Ord` for proxies.
- Implement `Default` for bitfield enums.
- Specialcase `wl_display` to be private.
