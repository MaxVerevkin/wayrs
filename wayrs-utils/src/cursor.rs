//! A simple but opinionated xcursor helper.
//!
//! # Example
//!
//! ```no_run
//! # use wayrs_utils::cursor::*;
//! # use wayrs_client::connection::Connection;
//! # let mut conn = Connection::<()>::connect().unwrap();
//! # let conn = &mut conn;
//! # let shm = todo!();
//! # let surface_scale = todo!();
//! # let enter_serial = todo!();
//! # let pointer = todo!();
//! # let wl_compositor = todo!();
//! #
//! // Do this once
//! let cursor_theme = CursorTheme::new(None, None);
//! let default_cursor = cursor_theme.get_image("default").unwrap();
//!
//! // Do this when you bind a pointer
//! let themed_pointer = ThemedPointer::new(conn, pointer, wl_compositor);
//!
//! // Set cursor (on `wl_pointer.enter` or whenever you need to)
//! themed_pointer.set_cursor(conn, shm, &default_cursor, surface_scale, enter_serial);
//!
//! ```

use std::io;

use wayrs_client::protocol::*;
use wayrs_client::{connection::Connection, proxy::Proxy};

use crate::shm_alloc::{BufferSpec, ShmAlloc};

use xcursor::parser::Image;

#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("cursor not found")]
    CursorNotFound,
    #[error("theme could not be parsed")]
    ThemeParseError,
    #[error(transparent)]
    ReadError(#[from] io::Error),
}

/// An easy to use xcursor theme wrapper.
pub struct CursorTheme {
    cursor_size: u32,
    theme: xcursor::CursorTheme,
}

/// A cursor image.
pub struct CursorImage {
    cursor_size: u32,
    imgs: Vec<Image>,
}

/// A wrapper around [`WlPointer`] with convenient [`set_cursor`](Self::set_cursor) and
/// [`hide_cursor`](Self::hide_cursor) methods.
pub struct ThemedPointer {
    pointer: WlPointer,
    surface: WlSurface,
}

impl CursorTheme {
    /// Load a cursor theme.
    ///
    /// `theme_name` defaults to `XCURSOR_THEME` env variable or `default`.
    ///
    /// `cursor_size` defaults to `XCURSOR_SIZE` env variable or `24`.
    pub fn new(theme_name: Option<&str>, cursor_size: Option<u32>) -> Self {
        let mut theme_name_buf = None;

        let theme_name = theme_name
            .or_else(|| {
                theme_name_buf = std::env::var("XCURSOR_THEME").ok();
                theme_name_buf.as_deref()
            })
            .unwrap_or("default");

        let cursor_size = cursor_size
            .or_else(|| {
                std::env::var("XCURSOR_SIZE")
                    .ok()
                    .and_then(|x| x.parse().ok())
            })
            .unwrap_or(24);

        let theme = xcursor::CursorTheme::load(theme_name);

        CursorTheme { cursor_size, theme }
    }

    /// Find and parse a cursor image.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use wayrs_utils::cursor::CursorTheme;
    /// let theme = CursorTheme::new(None, None);
    /// let default_cursor = theme.get_image("default");
    /// let move_cursor = theme.get_image("move");
    /// ```
    pub fn get_image(&self, cursor: &str) -> Result<CursorImage, CursorError> {
        let theme_path = self
            .theme
            .load_icon(cursor)
            .ok_or(CursorError::CursorNotFound)?;

        let raw_theme = std::fs::read(theme_path)?;

        let mut imgs =
            xcursor::parser::parse_xcursor(&raw_theme).ok_or(CursorError::ThemeParseError)?;
        if imgs.is_empty() {
            return Err(CursorError::CursorNotFound);
        }

        imgs.sort_unstable_by_key(|img| img.size);

        Ok(CursorImage {
            cursor_size: self.cursor_size,
            imgs,
        })
    }
}

impl ThemedPointer {
    /// Create new pointer wrapper.
    pub fn new<D>(
        conn: &mut Connection<D>,
        pointer: WlPointer,
        wl_compositor: WlCompositor,
    ) -> Self {
        Self {
            pointer,
            surface: wl_compositor.create_surface(conn),
        }
    }

    /// Set cursor image.
    ///
    /// Refer to [`WlPointer::set_cursor`] for more info.
    pub fn set_cursor<D>(
        &self,
        conn: &mut Connection<D>,
        shm: &mut ShmAlloc,
        image: &CursorImage,
        scale: u32,
        serial: u32,
    ) {
        let target_size = image.cursor_size * scale;

        let image = match image
            .imgs
            .binary_search_by_key(&target_size, |img| img.size)
        {
            Ok(indx) => &image.imgs[indx],
            Err(indx) if indx == 0 => image.imgs.first().unwrap(),
            Err(indx) if indx >= image.imgs.len() => image.imgs.last().unwrap(),
            Err(indx) => {
                let a = &image.imgs[indx - 1];
                let b = &image.imgs[indx];
                if target_size - a.size < b.size - target_size {
                    a
                } else {
                    b
                }
            }
        };

        let (buffer, canvas) = shm.alloc_buffer(
            conn,
            BufferSpec {
                width: image.width,
                height: image.height,
                stride: image.width * 4,
                format: wl_shm::Format::Argb8888,
            },
        );

        assert_eq!(image.pixels_rgba.len(), canvas.len());
        canvas.copy_from_slice(&image.pixels_rgba);

        self.surface.attach(conn, buffer.into_wl_buffer(), 0, 0);
        self.surface.damage(conn, 0, 0, i32::MAX, i32::MAX);
        self.surface.set_buffer_scale(conn, scale as i32);
        self.surface.commit(conn);

        self.pointer.set_cursor(
            conn,
            serial,
            self.surface,
            (image.xhot / scale) as i32,
            (image.yhot / scale) as i32,
        );
    }

    /// Hide cursor.
    ///
    /// Sets surface to NULL.
    pub fn hide_cursor<D>(&self, conn: &mut Connection<D>, serial: u32) {
        self.pointer
            .set_cursor(conn, serial, WlSurface::null(), 0, 0);
    }

    /// Destroy cursor's surface.
    pub fn destroy<D>(self, conn: &mut Connection<D>) {
        self.surface.destroy(conn);
    }
}
