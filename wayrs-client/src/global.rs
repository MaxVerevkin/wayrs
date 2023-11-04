use std::ffi::{CStr, CString};
use std::ops;

use crate::protocol::wl_registry::GlobalArgs;
use crate::proxy::Proxy;
use crate::{Connection, EventCtx};

pub type Global = GlobalArgs;
pub type Globals = [Global];

#[derive(Debug, thiserror::Error)]
pub enum BindError {
    #[error("global has interface {actual:?} but {requested:?} was requested")]
    IncorrectInterface {
        actual: CString,
        requested: &'static CStr,
    },
    #[error("global has version {actual} but a minimum version of {min} was requested")]
    UnsupportedVersion { actual: u32, min: u32 },
    #[error("global with interface {0:?} not found")]
    GlobalNotFound(&'static CStr),
}

pub trait GlobalExt {
    fn is<P: Proxy>(&self) -> bool;

    /// Bind a global.
    ///
    /// The version argmuent can be a:
    /// - Number - require a specific version
    /// - Full range (`..` - bind any version)
    /// - Range from (`a..` - require a version of at least `a`)
    /// - Range to inclusive (`..=b` - bind a version in range `[1, b]`)
    /// - Range inclusive (`a..=b` - bind a version in range `[a, b]`)
    fn bind<P: Proxy, D>(
        &self,
        conn: &mut Connection<D>,
        version: impl VersionBounds,
    ) -> Result<P, BindError>;

    /// Same as [`bind`](Self::bind) but also sets the callback
    fn bind_with_cb<P: Proxy, D, F: FnMut(EventCtx<D, P>) + Send + 'static>(
        &self,
        conn: &mut Connection<D>,
        version: impl VersionBounds,
        cb: F,
    ) -> Result<P, BindError>;
}

pub trait GlobalsExt {
    fn bind<P: Proxy, D>(
        &self,
        conn: &mut Connection<D>,
        version: impl VersionBounds,
    ) -> Result<P, BindError>;

    /// Same as [`bind`](Self::bind) but also sets the callback
    fn bind_with_cb<P: Proxy, D, F: FnMut(EventCtx<D, P>) + Send + 'static>(
        &self,
        conn: &mut Connection<D>,
        version: impl VersionBounds,
        cb: F,
    ) -> Result<P, BindError>;
}

impl GlobalExt for Global {
    fn is<P: Proxy>(&self) -> bool {
        P::INTERFACE.name == self.interface.as_c_str()
    }

    /// Bind the first instance of a global. Works great for singletons.
    ///
    /// The version argmuent can be a:
    /// - Number - require a specific version
    /// - Full range (`..` - bind any version)
    /// - Range from (`a..` - require a version of at least `a`)
    /// - Range to inclusive (`..=b` - bind a version in range `[1, b]`)
    /// - Range inclusive (`a..=b` - bind a version in range `[a, b]`)
    fn bind<P: Proxy, D>(
        &self,
        conn: &mut Connection<D>,
        version: impl VersionBounds,
    ) -> Result<P, BindError> {
        if !self.is::<P>() {
            return Err(BindError::IncorrectInterface {
                actual: self.interface.to_owned(),
                requested: P::INTERFACE.name,
            });
        }

        if let Some(upper) = version.upper() {
            assert!(upper <= P::INTERFACE.version);
        }

        if self.version < version.lower() {
            return Err(BindError::UnsupportedVersion {
                actual: self.version,
                min: version.lower(),
            });
        }

        let reg = conn.registry();
        let upper = version.upper().unwrap_or(P::INTERFACE.version);
        let version = u32::min(upper, self.version);

        Ok(reg.bind(conn, self.name, version))
    }

    /// Same as [`bind`](Self::bind) but also sets the callback
    fn bind_with_cb<P: Proxy, D, F: FnMut(EventCtx<D, P>) + Send + 'static>(
        &self,
        conn: &mut Connection<D>,
        version: impl VersionBounds,
        cb: F,
    ) -> Result<P, BindError> {
        if !self.is::<P>() {
            return Err(BindError::IncorrectInterface {
                actual: self.interface.to_owned(),
                requested: P::INTERFACE.name,
            });
        }

        if let Some(upper) = version.upper() {
            assert!(upper <= P::INTERFACE.version);
        }

        if self.version < version.lower() {
            return Err(BindError::UnsupportedVersion {
                actual: self.version,
                min: version.lower(),
            });
        }

        let reg = conn.registry();
        let version = match version.upper() {
            None => self.version,
            Some(upper) => u32::min(upper, self.version),
        };

        Ok(reg.bind_with_cb(conn, self.name, version, cb))
    }
}

impl GlobalsExt for Globals {
    fn bind<P: Proxy, D>(
        &self,
        conn: &mut Connection<D>,
        version: impl VersionBounds,
    ) -> Result<P, BindError> {
        let global = self
            .iter()
            .find(|g| g.is::<P>())
            .ok_or(BindError::GlobalNotFound(P::INTERFACE.name))?;
        global.bind(conn, version)
    }

    fn bind_with_cb<P: Proxy, D, F: FnMut(EventCtx<D, P>) + Send + 'static>(
        &self,
        conn: &mut Connection<D>,
        version: impl VersionBounds,
        cb: F,
    ) -> Result<P, BindError> {
        let global = self
            .iter()
            .find(|g| g.is::<P>())
            .ok_or(BindError::GlobalNotFound(P::INTERFACE.name))?;
        global.bind_with_cb(conn, version, cb)
    }
}

pub trait VersionBounds: private::Sealed {
    fn lower(&self) -> u32;
    fn upper(&self) -> Option<u32>;
}

mod private {
    pub trait Sealed {}
}

macro_rules! impl_version_bounds {
    ($($ty:ty => ($self:ident) => $lower:expr, $upper:expr;)*) => {
        $(
            impl private::Sealed for $ty {}
            impl VersionBounds for $ty {
                fn lower(&$self) -> u32 {
                    $lower
                }
                fn upper(&$self) -> Option<u32> {
                    $upper
                }
            }
        )*
    };
}

impl_version_bounds! [
    u32 => (self) => *self, Some(*self);
    ops::RangeFull => (self) => 1, None;
    ops::RangeFrom<u32> => (self) => self.start, None;
    ops::RangeToInclusive<u32> => (self) => 1, Some(self.end);
    ops::RangeInclusive<u32> => (self) => *self.start(), Some(*self.end());
];
