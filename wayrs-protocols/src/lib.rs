//! A collection of Wayland protocols to use with `wayrs_client`.
//!
//! All protocols are behind feature gates and none of them are enabeled by default.

#![cfg_attr(docsrs, feature(doc_cfg))]

macro_rules! gen {
    (mod: $mod:ident, feat: $feat:literal, file: $file:literal, deps: [$($dep:ident),*],) => {
        #[cfg(feature = $feat)]
        #[cfg_attr(docsrs, doc(cfg(feature = $feat)))]
        pub mod $mod {
            $(gen!(@dep $dep);)*
            wayrs_client::generate!($file);
        }
    };
    (@dep core) => {
        use wayrs_client::protocol::*;
    };
    (@dep $dep:ident) => {
        use super::$dep::*;
    };
}

gen! {
    mod: linux_dmabuf_v1,
    feat: "linux-dmabuf-v1",
    file: "wayland-protocols/stable/linux-dmabuf/linux-dmabuf-v1.xml",
    deps: [core],
}

gen! {
    mod: presentation_time,
    feat: "presentation-time",
    file: "wayland-protocols/stable/presentation-time/presentation-time.xml",
    deps: [core],
}

gen! {
    mod: viewporter,
    feat: "viewporter",
    file: "wayland-protocols/stable/viewporter/viewporter.xml",
    deps: [core],
}

gen! {
    mod: xdg_shell,
    feat: "xdg-shell",
    file: "wayland-protocols/stable/xdg-shell/xdg-shell.xml",
    deps: [core],
}

gen! {
    mod: alpha_modifier_v1,
    feat: "alpha-modifier-v1",
    file: "wayland-protocols/staging/alpha-modifier/alpha-modifier-v1.xml",
    deps: [core],
}

gen! {
    mod: content_type_v1,
    feat: "content-type-v1",
    file: "wayland-protocols/staging/content-type/content-type-v1.xml",
    deps: [core],
}

gen! {
    mod: cursor_shape_v1,
    feat: "cursor-shape-v1",
    file: "wayland-protocols/staging/cursor-shape/cursor-shape-v1.xml",
    deps: [core, tablet_v2],
}

gen! {
    mod: drm_lease_v1,
    feat: "drm-lease-v1",
    file: "wayland-protocols/staging/drm-lease/drm-lease-v1.xml",
    deps: [],
}

gen! {
    mod: ext_foreign_toplevel_list,
    feat: "ext-foreign-toplevel-list-v1",
    file: "wayland-protocols/staging/ext-foreign-toplevel-list/ext-foreign-toplevel-list-v1.xml",
    deps: [],
}

gen! {
    mod: ext_idle_notify_v1,
    feat: "ext-idle-notify-v1",
    file: "wayland-protocols/staging/ext-idle-notify/ext-idle-notify-v1.xml",
    deps: [core],
}

gen! {
    mod: ext_session_lock_v1,
    feat: "ext-session-lock-v1",
    file: "wayland-protocols/staging/ext-session-lock/ext-session-lock-v1.xml",
    deps: [core],
}

gen! {
    mod: ext_transient_seat_v1,
    feat: "ext-transient-seat-v1",
    file: "wayland-protocols/staging/ext-transient-seat/ext-transient-seat-v1.xml",
    deps: [],
}

gen! {
    mod: fractional_scale_v1,
    feat: "fractional-scale-v1",
    file: "wayland-protocols/staging/fractional-scale/fractional-scale-v1.xml",
    deps: [core],
}

gen! {
    mod: linux_drm_syncobj_v1,
    feat: "linux-drm-syncobj-v1",
    file: "wayland-protocols/staging/linux-drm-syncobj/linux-drm-syncobj-v1.xml",
    deps: [core],
}

gen! {
    mod: security_context_v1,
    feat: "security-context-v1",
    file: "wayland-protocols/staging/security-context/security-context-v1.xml",
    deps: [],
}

gen! {
    mod: single_pixel_buffer_v1,
    feat: "single-pixel-buffer-v1",
    file: "wayland-protocols/staging/single-pixel-buffer/single-pixel-buffer-v1.xml",
    deps: [core],
}

gen! {
    mod: tearing_control_v1,
    feat: "tearing-control-v1",
    file: "wayland-protocols/staging/tearing-control/tearing-control-v1.xml",
    deps: [core],
}

gen! {
    mod: xdg_activation_v1,
    feat: "xdg-activation-v1",
    file: "wayland-protocols/staging/xdg-activation/xdg-activation-v1.xml",
    deps: [core],
}

gen! {
    mod: xdg_dialog_v1,
    feat: "xdg-dialog-v1",
    file: "wayland-protocols/staging/xdg-dialog/xdg-dialog-v1.xml",
    deps: [xdg_shell],
}

gen! {
    mod: xdg_toplevel_drag_v1,
    feat: "xdg-toplevel-drag-v1",
    file: "wayland-protocols/staging/xdg-toplevel-drag/xdg-toplevel-drag-v1.xml",
    deps: [core, xdg_shell],
}

gen! {
    mod: xwayland_shell_v1,
    feat: "xwayland-shell-v1",
    file: "wayland-protocols/staging/xwayland-shell/xwayland-shell-v1.xml",
    deps: [core],
}

gen! {
    mod: fullscreen_shell_unstable_v1,
    feat: "fullscreen-shell-unstable-v1",
    file: "wayland-protocols/unstable/fullscreen-shell/fullscreen-shell-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: idle_inhibit_unstable_v1,
    feat: "idle-inhibit-unstable-v1",
    file: "wayland-protocols/unstable/idle-inhibit/idle-inhibit-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: input_method_unstable_v1,
    feat: "input-method-unstable-v1",
    file: "wayland-protocols/unstable/input-method/input-method-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: input_timestamps_unstable_v1,
    feat: "input-timestamps-unstable-v1",
    file: "wayland-protocols/unstable/input-timestamps/input-timestamps-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: keyboard_shortcuts_inhibit_unstable_v1,
    feat: "keyboard-shortcuts-inhibit-unstable-v1",
    file: "wayland-protocols/unstable/keyboard-shortcuts-inhibit/keyboard-shortcuts-inhibit-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: linux_explicit_synchronization_unstable_v1,
    feat: "linux-explicit-synchronization-unstable-v1",
    file: "wayland-protocols/unstable/linux-explicit-synchronization/linux-explicit-synchronization-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: pointer_constraints_unstable_v1,
    feat: "pointer-constraints-unstable-v1",
    file: "wayland-protocols/unstable/pointer-constraints/pointer-constraints-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: pointer_gestures_unstable_v1,
    feat: "pointer-gestures-unstable-v1",
    file: "wayland-protocols/unstable/pointer-gestures/pointer-gestures-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: primary_selection_unstable_v1,
    feat: "primary-selection-unstable-v1",
    file: "wayland-protocols/unstable/primary-selection/primary-selection-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: relative_pointer_unstable_v1,
    feat: "relative-pointer-unstable-v1",
    file: "wayland-protocols/unstable/relative-pointer/relative-pointer-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: tablet_unstable_v1,
    feat: "tablet-unstable-v1",
    file: "wayland-protocols/unstable/tablet/tablet-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: tablet_v2,
    feat: "tablet-v2",
    file: "wayland-protocols/stable/tablet/tablet-v2.xml",
    deps: [core],
}

gen! {
    mod: text_input_unstable_v1,
    feat: "text-input-unstable-v1",
    file: "wayland-protocols/unstable/text-input/text-input-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: text_input_unstable_v3,
    feat: "text-input-unstable-v3",
    file: "wayland-protocols/unstable/text-input/text-input-unstable-v3.xml",
    deps: [core],
}

gen! {
    mod: xdg_decoration_unstable_v1,
    feat: "xdg-decoration-unstable-v1",
    file: "wayland-protocols/unstable/xdg-decoration/xdg-decoration-unstable-v1.xml",
    deps: [xdg_shell],
}

gen! {
    mod: xdg_foreign_unstable_v1,
    feat: "xdg-foreign-unstable-v1",
    file: "wayland-protocols/unstable/xdg-foreign/xdg-foreign-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: xdg_foreign_unstable_v2,
    feat: "xdg-foreign-unstable-v2",
    file: "wayland-protocols/unstable/xdg-foreign/xdg-foreign-unstable-v2.xml",
    deps: [core],
}

gen! {
    mod: xdg_output_unstable_v1,
    feat: "xdg-output-unstable-v1",
    file: "wayland-protocols/unstable/xdg-output/xdg-output-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: xdg_shell_unstable_v5,
    feat: "xdg-shell-unstable-v5",
    file: "wayland-protocols/unstable/xdg-shell/xdg-shell-unstable-v5.xml",
    deps: [core],
}

gen! {
    mod: xdg_shell_unstable_v6,
    feat: "xdg-shell-unstable-v6",
    file: "wayland-protocols/unstable/xdg-shell/xdg-shell-unstable-v6.xml",
    deps: [core],
}

gen! {
    mod: xwayland_keyboard_grab_unstable_v1,
    feat: "xwayland-keyboard-grab-unstable-v1",
    file: "wayland-protocols/unstable/xwayland-keyboard-grab/xwayland-keyboard-grab-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: wlr_data_control_unstable_v1,
    feat: "wlr-data-control-unstable-v1",
    file: "wlr-protocols/unstable/wlr-data-control-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: wlr_export_dmabuf_unstable_v1,
    feat: "wlr-export-dmabuf-unstable-v1",
    file: "wlr-protocols/unstable/wlr-export-dmabuf-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: wlr_foreign_toplevel_management_unstable_v1,
    feat: "wlr-foreign-toplevel-management-unstable-v1",
    file: "wlr-protocols/unstable/wlr-foreign-toplevel-management-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: wlr_gamma_control_unstable_v1,
    feat: "wlr-gamma-control-unstable-v1",
    file: "wlr-protocols/unstable/wlr-gamma-control-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: wlr_input_inhibitor_unstable_v1,
    feat: "wlr-input-inhibitor-unstable-v1",
    file: "wlr-protocols/unstable/wlr-input-inhibitor-unstable-v1.xml",
    deps: [],
}

gen! {
    mod: wlr_layer_shell_unstable_v1,
    feat: "wlr-layer-shell-unstable-v1",
    file: "wlr-protocols/unstable/wlr-layer-shell-unstable-v1.xml",
    deps: [core, xdg_shell],
}

gen! {
    mod: wlr_output_management_unstable_v1,
    feat: "wlr-output-management-unstable-v1",
    file: "wlr-protocols/unstable/wlr-output-management-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: wlr_output_power_management_unstable_v1,
    feat: "wlr-output-power-management-unstable-v1",
    file: "wlr-protocols/unstable/wlr-output-power-management-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: wlr_screencopy_unstable_v1,
    feat: "wlr-screencopy-unstable-v1",
    file: "wlr-protocols/unstable/wlr-screencopy-unstable-v1.xml",
    deps: [core],
}

gen! {
    mod: wlr_virtual_pointer_unstable_v1,
    feat: "wlr-virtual-pointer-unstable-v1",
    file: "wlr-protocols/unstable/wlr-virtual-pointer-unstable-v1.xml",
    deps: [core],
}
