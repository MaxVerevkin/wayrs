use std::ffi::{CStr, CString};
use std::ops::RangeInclusive;

use crate::protocol::wl_registry::GlobalArgs;
use crate::proxy::Proxy;
use crate::Connection;

pub type Global = GlobalArgs;
pub type Globals = [Global];

pub trait GlobalExt {
    fn is<P: Proxy>(&self) -> bool;

    fn bind<P: Proxy, D>(
        &self,
        conn: &mut Connection<D>,
        version: RangeInclusive<u32>,
    ) -> Result<P, BindError>;

    fn bind_with_cb<
        P: Proxy,
        D,
        F: FnMut(&mut Connection<D>, &mut D, P, P::Event) + Send + 'static,
    >(
        &self,
        conn: &mut Connection<D>,
        version: RangeInclusive<u32>,
        cb: F,
    ) -> Result<P, BindError>;
}

pub trait GlobalsExt {
    fn bind<P: Proxy, D>(
        &self,
        conn: &mut Connection<D>,
        version: RangeInclusive<u32>,
    ) -> Result<P, BindError>;

    fn bind_with_cb<
        P: Proxy,
        D,
        F: FnMut(&mut Connection<D>, &mut D, P, P::Event) + Send + 'static,
    >(
        &self,
        conn: &mut Connection<D>,
        version: RangeInclusive<u32>,
        cb: F,
    ) -> Result<P, BindError>;
}

impl GlobalExt for Global {
    fn is<P: Proxy>(&self) -> bool {
        P::INTERFACE.name == self.interface.as_c_str()
    }

    fn bind<P: Proxy, D>(
        &self,
        conn: &mut Connection<D>,
        version: RangeInclusive<u32>,
    ) -> Result<P, BindError> {
        if !self.is::<P>() {
            return Err(BindError::IncorrectInterface {
                actual: self.interface.to_owned(),
                requested: P::INTERFACE.name,
            });
        }

        assert!(*version.end() <= P::INTERFACE.version);

        if self.version < *version.start() {
            return Err(BindError::UnsupportedVersion {
                actual: self.version,
                min: *version.start(),
                max: *version.end(),
            });
        }

        let reg = conn.registry();
        let version = u32::min(*version.end(), self.version);

        Ok(reg.bind(conn, self.name, version))
    }

    fn bind_with_cb<
        P: Proxy,
        D,
        F: FnMut(&mut Connection<D>, &mut D, P, P::Event) + Send + 'static,
    >(
        &self,
        conn: &mut Connection<D>,
        version: RangeInclusive<u32>,
        cb: F,
    ) -> Result<P, BindError> {
        if !self.is::<P>() {
            return Err(BindError::IncorrectInterface {
                actual: self.interface.to_owned(),
                requested: P::INTERFACE.name,
            });
        }

        assert!(*version.end() <= P::INTERFACE.version);

        if self.version < *version.start() {
            return Err(BindError::UnsupportedVersion {
                actual: self.version,
                min: *version.start(),
                max: *version.end(),
            });
        }

        let reg = conn.registry();
        let version = u32::min(*version.end(), self.version);

        Ok(reg.bind_with_cb(conn, self.name, version, cb))
    }
}

impl GlobalsExt for Globals {
    fn bind<P: Proxy, D>(
        &self,
        conn: &mut Connection<D>,
        version: RangeInclusive<u32>,
    ) -> Result<P, BindError> {
        let global = self
            .iter()
            .find(|g| g.is::<P>())
            .ok_or(BindError::GlobalNotFound(P::INTERFACE.name))?;
        global.bind(conn, version)
    }

    fn bind_with_cb<
        P: Proxy,
        D,
        F: FnMut(&mut Connection<D>, &mut D, P, P::Event) + Send + 'static,
    >(
        &self,
        conn: &mut Connection<D>,
        version: RangeInclusive<u32>,
        cb: F,
    ) -> Result<P, BindError> {
        let global = self
            .iter()
            .find(|g| g.is::<P>())
            .ok_or(BindError::GlobalNotFound(P::INTERFACE.name))?;
        global.bind_with_cb(conn, version, cb)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BindError {
    #[error("global has interface {actual:?} but {requested:?} was requested")]
    IncorrectInterface {
        actual: CString,
        requested: &'static CStr,
    },
    #[error("global has version {actual} but version in range [{min}, {max}] was requested")]
    UnsupportedVersion { actual: u32, min: u32, max: u32 },
    #[error("global with interface {0:?} not found")]
    GlobalNotFound(&'static CStr),
}
