//! A collection of Wayland protocols to use with `wayrs_client`.
//!
//! All protocols are behind feature gates and none of them are enabeled by default. Check out
//! [Cargo.toml](https://github.com/MaxVerevkin/wayrs/blob/main/wayrs-protocols/Cargo.toml) for a
//! list of available features.

#[cfg(feature = "xdg-shell")]
pub mod xdg_shell {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wayland-protocols/stable/xdg-shell/xdg-shell.xml");
}

#[cfg(feature = "viewporter")]
pub mod viewporter {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wayland-protocols/stable/viewporter/viewporter.xml");
}

#[cfg(feature = "presentation-time")]
pub mod presentation_time {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/stable/presentation-time/presentation-time.xml"
    );
}

#[cfg(feature = "content-type-v1")]
pub mod content_type_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wayland-protocols/staging/content-type/content-type-v1.xml");
}

#[cfg(feature = "drm-lease-v1")]
pub mod drm_lease_v1 {
    use wayrs_client;
    wayrs_client::scanner::generate!("wayland-protocols/staging/drm-lease/drm-lease-v1.xml");
}

#[cfg(feature = "ext-foreign-toplevel-list-v1")]
pub mod ext_foreign_toplevel_list {
    use wayrs_client;
    wayrs_client::scanner::generate!(
        "wayland-protocols/staging/ext-foreign-toplevel-list/ext-foreign-toplevel-list-v1.xml"
    );
}

#[cfg(feature = "ext-idle-notify-v1")]
pub mod ext_idle_notify_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/staging/ext-idle-notify/ext-idle-notify-v1.xml"
    );
}

#[cfg(feature = "ext-session-lock-v1")]
pub mod ext_session_lock_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/staging/ext-session-lock/ext-session-lock-v1.xml"
    );
}

#[cfg(feature = "fractional-scale-v1")]
pub mod fractional_scale_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/staging/fractional-scale/fractional-scale-v1.xml"
    );
}

#[cfg(feature = "single-pixel-buffer-v1")]
pub mod single_pixel_buffer_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/staging/single-pixel-buffer/single-pixel-buffer-v1.xml"
    );
}

#[cfg(feature = "tearing-control-v1")]
pub mod tearing_control_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/staging/tearing-control/tearing-control-v1.xml"
    );
}

#[cfg(feature = "xdg-activation-v1")]
pub mod xdg_activation_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/staging/xdg-activation/xdg-activation-v1.xml"
    );
}

#[cfg(feature = "xwayland-shell-v1")]
pub mod xwayland_shell_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/staging/xwayland-shell/xwayland-shell-v1.xml"
    );
}

#[cfg(feature = "fullscreen-shell-unstable-v1")]
pub mod fullscreen_shell_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/fullscreen-shell/fullscreen-shell-unstable-v1.xml"
    );
}

#[cfg(feature = "idle-inhibit-unstable-v1")]
pub mod idle_inhibit_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/idle-inhibit/idle-inhibit-unstable-v1.xml"
    );
}

#[cfg(feature = "input-timestamps-unstable-v1")]
pub mod input_timestamps_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/input-timestamps/input-timestamps-unstable-v1.xml"
    );
}

#[cfg(feature = "keyboard-shortcuts-inhibit-unstable-v1")]
pub mod keyboard_shortcuts_inhibit_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/keyboard-shortcuts-inhibit/keyboard-shortcuts-inhibit-unstable-v1.xml"
    );
}

#[cfg(feature = "linux-dmabuf-unstable-v1")]
pub mod linux_dmabuf_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/linux-dmabuf/linux-dmabuf-unstable-v1.xml"
    );
}

#[cfg(feature = "linux-explicit-synchronization-unstable-v1")]
pub mod linux_explicit_synchronization_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/linux-explicit-synchronization/linux-explicit-synchronization-unstable-v1.xml"
    );
}

#[cfg(feature = "pointer-constraints-unstable-v1")]
pub mod pointer_constraints_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/pointer-constraints/pointer-constraints-unstable-v1.xml"
    );
}

#[cfg(feature = "pointer-gestures-unstable-v1")]
pub mod pointer_gestures_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/pointer-gestures/pointer-gestures-unstable-v1.xml"
    );
}

#[cfg(feature = "primary-selection-unstable-v1")]
pub mod primary_selection_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/primary-selection/primary-selection-unstable-v1.xml"
    );
}

#[cfg(feature = "relative-pointer-unstable-v1")]
pub mod relative_pointer_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/relative-pointer/relative-pointer-unstable-v1.xml"
    );
}

#[cfg(feature = "tablet-unstable-v1")]
pub mod tablet_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wayland-protocols/unstable/tablet/tablet-unstable-v1.xml");
}

#[cfg(feature = "tablet-unstable-v2")]
pub mod tablet_unstable_v2 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wayland-protocols/unstable/tablet/tablet-unstable-v2.xml");
}

#[cfg(feature = "text-input-unstable-v1")]
pub mod text_input_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/text-input/text-input-unstable-v1.xml"
    );
}

#[cfg(feature = "text-input-unstable-v3")]
pub mod text_input_unstable_v3 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/text-input/text-input-unstable-v3.xml"
    );
}

#[cfg(feature = "xdg-decoration-unstable-v1")]
pub mod xdg_decoration_unstable_v1 {
    use super::xdg_shell::*;
    use wayrs_client;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/xdg-decoration/xdg-decoration-unstable-v1.xml"
    );
}

#[cfg(feature = "xdg-foreign-unstable-v1")]
pub mod xdg_foreign_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/xdg-foreign/xdg-foreign-unstable-v1.xml"
    );
}

#[cfg(feature = "xdg-foreign-unstable-v2")]
pub mod xdg_foreign_unstable_v2 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/xdg-foreign/xdg-foreign-unstable-v2.xml"
    );
}

#[cfg(feature = "xdg-output-unstable-v1")]
pub mod xdg_output_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/xdg-output/xdg-output-unstable-v1.xml"
    );
}

#[cfg(feature = "xdg-shell-unstable-v5")]
pub mod xdg_shell_unstable_v5 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/xdg-shell/xdg-shell-unstable-v5.xml"
    );
}

#[cfg(feature = "xdg-shell-unstable-v6")]
pub mod xdg_shell_unstable_v6 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/xdg-shell/xdg-shell-unstable-v6.xml"
    );
}

#[cfg(feature = "xwayland-keyboard-grab-unstable-v1")]
pub mod xwayland_keyboard_grab_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wayland-protocols/unstable/xwayland-keyboard-grab/xwayland-keyboard-grab-unstable-v1.xml"
    );
}

#[cfg(feature = "wlr-data-control-unstable-v1")]
pub mod wlr_data_control_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wlr-protocols/unstable/wlr-data-control-unstable-v1.xml");
}

#[cfg(feature = "wlr-export-dmabuf-unstable-v1")]
pub mod wlr_export_dmabuf_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wlr-protocols/unstable/wlr-export-dmabuf-unstable-v1.xml");
}

#[cfg(feature = "wlr-foreign-toplevel-management-unstable-v1")]
pub mod wlr_foreign_toplevel_management_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wlr-protocols/unstable/wlr-foreign-toplevel-management-unstable-v1.xml"
    );
}

#[cfg(feature = "wlr-gamma-control-unstable-v1")]
pub mod wlr_gamma_control_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wlr-protocols/unstable/wlr-gamma-control-unstable-v1.xml");
}

#[cfg(feature = "wlr-input-inhibitor-unstable-v1")]
pub mod wlr_input_inhibitor_unstable_v1 {
    use wayrs_client;
    wayrs_client::scanner::generate!("wlr-protocols/unstable/wlr-input-inhibitor-unstable-v1.xml");
}

#[cfg(feature = "wlr-layer-shell-unstable-v1")]
pub mod wlr_layer_shell_unstable_v1 {
    use super::xdg_shell::*;
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wlr-protocols/unstable/wlr-layer-shell-unstable-v1.xml");
}

#[cfg(feature = "wlr-output-management-unstable-v1")]
pub mod wlr_output_management_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wlr-protocols/unstable/wlr-output-management-unstable-v1.xml"
    );
}

#[cfg(feature = "wlr-output-power-management-unstable-v1")]
pub mod wlr_output_power_management_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!(
        "wlr-protocols/unstable/wlr-output-power-management-unstable-v1.xml"
    );
}

#[cfg(feature = "wlr-screencopy-unstable-v1")]
pub mod wlr_screencopy_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wlr-protocols/unstable/wlr-screencopy-unstable-v1.xml");
}

#[cfg(feature = "wlr-virtual-pointer-unstable-v1")]
pub mod wlr_virtual_pointer_unstable_v1 {
    use wayrs_client;
    use wayrs_client::protocol::*;
    wayrs_client::scanner::generate!("wlr-protocols/unstable/wlr-virtual-pointer-unstable-v1.xml");
}
