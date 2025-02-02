# 0.14.6+1.40

- Update `wayland-protocols` to v1.40.

# 0.14.5+1.39

- Update `wayland-protocols` to v1.39.
- New protocols: `ext-workspace-v1` and `ext-data-control-v1`.
- Include the `wayland-protocols` version in the crate version.

# 0.14.4

- Update `wayland-protocols` to v1.38.
- New protocols: `commit-timing-v1`, `fifo-v1` and `xdg-system-bell-v1`.

# 0.14.3

- Update `wayland-protocols` to v1.37.
- New protocols: `xdg-toplevel-icon-v1`, `ext-image-capture-source-v1` and `ext-image-copy-capture-v1`.

# 0.14.2

- Update `wayland-protocols` to v1.36 (nothing interesting).
- Update `wlr-protocols` to [2b8d433](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/commit/2b8d43325b7012cc3f9b55c08d26e50e42beac7d) (updates the layer shell protocol to v5).

# 0.14.1

- Include license file in the package.

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
