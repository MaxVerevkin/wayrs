# 0.15.0

- Update `wayrs-protocols` to v0.14.
- dmabuf_feedback: map format table with MAP_PRIVATE.
- shm_alloc: destroy wl_shm if is v2.
- shm_alloc: add `ShmAlloc::destroy`.

# 0.14.0

- `ShmAlloc::alloc_buffer` now returns `io::Result` and does not panic.

# 0.13.0

- Update to `wayrs-cilent` v1.

# 0.12.0

- Keyboard: mark `KeyboardEvent` as `non_exhaustive`.
- Keyboard: include focused surface id in `KeyboardEvent`.
- Dmabuf: make all `DmabufFeedback` fields private, add getters.
- Dmabuf: do not copy format table from memory-map (reinterpret mmap instead).

# 0.11.0

- Panic if `KeyboardHandler::get_keyboard` implementation is incorrect.
- Update `xkbcommon` to v0.7 and `memmap2` to v0.8.

# 0.10.0

- Update `xkbcommon` to v0.6.
- Use `wayrs-client` v0.12.

# 0.9.0

- Add `Timer`. Usefull for keyboard repeats.
- Keyboard: set `repeat_info` only on events that should be repeated (as defined by the current keymap).

# 0.8.0

- Make `CursorImageImp` private.
- `CursorTheme::new` now takes `WlCompositor` as an argument, instead of binding it on its own.

# 0.7.0

- Add `dmabuf-feedback` helper.
- Add an example to `seats` docs.
