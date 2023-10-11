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
