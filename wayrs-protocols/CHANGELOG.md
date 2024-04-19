# 0.14.0

- Update `wayland-protocols` to v1.35.
- New protocols: `xdg-toplevel-drag-v1`, `xdg-dialog-v1`, `linux-drm-syncobj-v1`, `alpha-modifier-v1`.
- `XdgPositioner::set_constraint_adjustment` now accepts the `ConstraintAdjustment` instead of `u32`.
- Replace `tablet-unstable-v2` with `tablet-v2`.
- Replace `linux-dmabuf-unstable-v1` with `linux-dmabuf-v1`.

# 0.13.2

- Add missing `input-method-unstable-v1` protocol.

# 0.13.1

- Update `wayland-protocols` to v1.33.
- Adds `ext-transient-seat-v1`.
- Adds `linux-dmabuf-v1` (supersedes `linux-dmabuf-unstable-v1`).

# 0.13.0

- Update to `wayrs-client` to v1.

# 0.12.1

- Release to trigger docs.rs to regenerate the documentation.

# 0.12.0

- Update `wayland-protocols` to [e1d61ce9](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/commit/e1d61ce9402ebd996d758c43f167e6280c1a3568).

# 0.11.0

- Update `wayland-protocols` to [5293896c](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/commit/5293896cce3e7a27b3d4e2212e5b2f36ac88b13a) (adds `security-context-v1`).
