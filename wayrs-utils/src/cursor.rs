//! A simple but opinionated xcursor helper.
//!
//! # Example
//!
//! ```ignore
//! // Do this once
//! let cursor_theme = CursorTheme::new(conn, &globals, wl_compositor);
//! let default_cursor = cursor_theme.get_image(CursorShape::Default).unwrap();
//!
//! // Do this when you bind a pointer
//! let themed_pointer = cursor_theme.get_themed_pointer(conn, pointer);
//!
//! // Set cursor (on `wl_pointer.enter` or whenever you need to)
//! themed_pointer.set_cursor(conn, shm, &default_cursor, surface_scale, enter_serial);
//!
//! ```

use std::{fmt, fs, io};

use wayrs_client::global::*;
use wayrs_client::object::Proxy;
use wayrs_client::protocol::*;
use wayrs_client::Connection;

use crate::shm_alloc::{BufferSpec, ShmAlloc};

use xcursor::parser::{parse_xcursor_stream, Image};

use wayrs_protocols::cursor_shape_v1::*;
pub use wp_cursor_shape_device_v1::Shape as CursorShape;

#[derive(Debug)]
pub enum CursorError {
    DefaultCursorNotFound,
    ThemeParseError,
    ReadError(io::Error),
}

impl std::error::Error for CursorError {}

impl fmt::Display for CursorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DefaultCursorNotFound => f.write_str("default cursor not found"),
            Self::ThemeParseError => f.write_str("theme could not be parsed"),
            Self::ReadError(error) => error.fmt(f),
        }
    }
}

impl From<io::Error> for CursorError {
    fn from(value: io::Error) -> Self {
        Self::ReadError(value)
    }
}

/// [`WpCursorShapeManagerV1`] wrapper which fallbacks to `xcursor` when `cursor-shape-v1` protocol
/// extension is not supported.
#[derive(Debug)]
pub struct CursorTheme(CursorThemeImp);

#[derive(Debug)]
enum CursorThemeImp {
    Server {
        manager: WpCursorShapeManagerV1,
    },
    Client {
        compositor: WlCompositor,
        cursor_size: u32,
        theme: xcursor::CursorTheme,
    },
}

/// A cursor image.
#[derive(Debug)]
pub struct CursorImage(CursorImageImp);

#[derive(Debug)]
enum CursorImageImp {
    Server { shape: CursorShape },
    Client { cursor_size: u32, imgs: Vec<Image> },
}

/// A wrapper around [`WlPointer`] with convenient [`set_cursor`](Self::set_cursor) and
/// [`hide_cursor`](Self::hide_cursor) methods.
#[derive(Debug)]
pub struct ThemedPointer {
    pointer: WlPointer,
    imp: ThemedPointerImp,
}

#[derive(Debug)]
enum ThemedPointerImp {
    Server { device: WpCursorShapeDeviceV1 },
    Client { surface: WlSurface },
}

impl CursorTheme {
    /// Create new [`CursorTheme`], preferring the server-side implementation if possible.
    pub fn new<D>(conn: &mut Connection<D>, globals: &Globals, compositor: WlCompositor) -> Self {
        match globals.bind(conn, 1..=1) {
            Ok(manager) => Self(CursorThemeImp::Server { manager }),
            Err(_) => {
                let theme = xcursor::CursorTheme::load(
                    std::env::var("XCURSOR_THEME")
                        .as_deref()
                        .unwrap_or("default"),
                );

                let cursor_size = std::env::var("XCURSOR_SIZE")
                    .ok()
                    .and_then(|x| x.parse().ok())
                    .unwrap_or(24);

                Self(CursorThemeImp::Client {
                    compositor,
                    cursor_size,
                    theme,
                })
            }
        }
    }

    /// Find and parse a cursor image.
    ///
    /// No-op if server-side implementation is used.
    pub fn get_image(&self, shape: CursorShape) -> Result<CursorImage, CursorError> {
        match &self.0 {
            CursorThemeImp::Server { .. } => Ok(CursorImage(CursorImageImp::Server { shape })),
            CursorThemeImp::Client {
                cursor_size, theme, ..
            } => {
                let theme_path = theme
                    .load_icon(stringify_cursor_shape(shape))
                    .or_else(|| theme.load_icon("default"))
                    .ok_or(CursorError::DefaultCursorNotFound)?;

                let mut reader = io::BufReader::new(fs::File::open(theme_path)?);
                let mut imgs = match parse_xcursor_stream(&mut reader) {
                    Ok(imgs) => imgs,
                    Err(e) if e.kind() == io::ErrorKind::Other => {
                        return Err(CursorError::ThemeParseError);
                    }
                    Err(e) => {
                        return Err(e.into());
                    }
                };

                if imgs.is_empty() {
                    return Err(CursorError::DefaultCursorNotFound);
                }

                imgs.sort_unstable_by_key(|img| img.size);

                Ok(CursorImage(CursorImageImp::Client {
                    cursor_size: *cursor_size,
                    imgs,
                }))
            }
        }
    }

    pub fn get_themed_pointer<D>(
        &self,
        conn: &mut Connection<D>,
        pointer: WlPointer,
    ) -> ThemedPointer {
        ThemedPointer {
            pointer,
            imp: match &self.0 {
                CursorThemeImp::Server { manager } => ThemedPointerImp::Server {
                    device: manager.get_pointer(conn, pointer),
                },
                CursorThemeImp::Client { compositor, .. } => ThemedPointerImp::Client {
                    surface: compositor.create_surface(conn),
                },
            },
        }
    }
}

impl ThemedPointer {
    /// Set cursor image.
    ///
    /// Refer to [`WlPointer::set_cursor`] for more info.
    ///
    /// `shm` and `scale` are ignored if server-side implementation is used.
    ///
    /// # Panics
    ///
    /// This function may panic if the [`CursorShape`] was created form different [`CursorTheme`]
    /// than this [`ThemedPointer`].
    pub fn set_cursor<D>(
        &self,
        conn: &mut Connection<D>,
        shm: &mut ShmAlloc,
        image: &CursorImage,
        scale: u32,
        serial: u32,
    ) {
        match (&self.imp, &image.0) {
            (ThemedPointerImp::Server { device }, CursorImageImp::Server { shape }) => {
                device.set_shape(conn, serial, *shape);
            }
            (
                ThemedPointerImp::Client { surface },
                CursorImageImp::Client { cursor_size, imgs },
            ) => {
                let scale = if surface.version() >= 3 { scale } else { 1 };
                let target_size = cursor_size * scale;

                let image = match imgs.binary_search_by_key(&target_size, |img| img.size) {
                    Ok(indx) => &imgs[indx],
                    Err(0) => imgs.first().unwrap(),
                    Err(indx) if indx >= imgs.len() => imgs.last().unwrap(),
                    Err(indx) => {
                        let a = &imgs[indx - 1];
                        let b = &imgs[indx];
                        if target_size - a.size < b.size - target_size {
                            a
                        } else {
                            b
                        }
                    }
                };

                let (buffer, canvas) = shm
                    .alloc_buffer(
                        conn,
                        BufferSpec {
                            width: image.width,
                            height: image.height,
                            stride: image.width * 4,
                            format: wl_shm::Format::Argb8888,
                        },
                    )
                    .expect("could not allocate frame shm buffer");

                assert_eq!(image.pixels_rgba.len(), canvas.len());
                canvas.copy_from_slice(&image.pixels_rgba);

                surface.attach(conn, Some(buffer.into_wl_buffer()), 0, 0);
                surface.damage(conn, 0, 0, i32::MAX, i32::MAX);
                if surface.version() >= 3 {
                    surface.set_buffer_scale(conn, scale as i32);
                }
                surface.commit(conn);

                self.pointer.set_cursor(
                    conn,
                    serial,
                    Some(*surface),
                    (image.xhot / scale) as i32,
                    (image.yhot / scale) as i32,
                );
            }
            _ => panic!("ThemedPointer and CursorImage implementation mismatch"),
        }
    }

    /// Hide cursor.
    ///
    /// Sets surface to NULL.
    pub fn hide_cursor<D>(&self, conn: &mut Connection<D>, serial: u32) {
        self.pointer.set_cursor(conn, serial, None, 0, 0);
    }

    /// Destroy cursor's surface / cursor shape device.
    ///
    /// This function does not destroy the pointer.
    pub fn destroy<D>(self, conn: &mut Connection<D>) {
        match &self.imp {
            ThemedPointerImp::Server { device } => device.destroy(conn),
            ThemedPointerImp::Client { surface } => surface.destroy(conn),
        }
    }
}

fn stringify_cursor_shape(shape: CursorShape) -> &'static str {
    const NAMES: &[&str] = &[
        "default",
        "context-menu",
        "help",
        "pointer",
        "progress",
        "wait",
        "cell",
        "crosshair",
        "text",
        "vertical-text",
        "alias",
        "copy",
        "move",
        "no-drop",
        "not-allowed",
        "grab",
        "grabbing",
        "e-resize",
        "n-resize",
        "ne-resize",
        "nw-resize",
        "s-resize",
        "se-resize",
        "sw-resize",
        "w-resize",
        "ew-resize",
        "ns-resize",
        "nesw-resize",
        "nwse-resize",
        "col-resize",
        "row-resize",
        "all-scroll",
        "zoom-in",
        "zoom-out",
    ];
    NAMES
        .get(u32::from(shape).saturating_sub(1) as usize)
        .unwrap_or(&"default")
}
