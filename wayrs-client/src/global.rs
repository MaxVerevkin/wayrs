use std::ffi::CString;
use std::{ffi::CStr, ops::RangeInclusive};

use crate::event_queue::EventQueue;
use crate::proxy::{Dispatch, Proxy};

pub type Global = crate::protocol::wl_registry::GlobalArgs;
pub type Globals = [Global];

pub trait GlobalExt {
    fn is<P: Proxy>(&self) -> bool;

    fn bind<P: Proxy, D: Dispatch<P>>(
        &self,
        event_queue: &mut EventQueue<D>,
        version: RangeInclusive<u32>,
    ) -> Result<P, BindError>;
}

pub trait GlobalsExt {
    fn bind<P: Proxy, D: Dispatch<P>>(
        &self,
        event_queue: &mut EventQueue<D>,
        version: RangeInclusive<u32>,
    ) -> Result<P, BindError>;
}

impl GlobalExt for Global {
    fn is<P: Proxy>(&self) -> bool {
        P::interface().name == self.interface.as_c_str()
    }

    fn bind<P: Proxy, D: Dispatch<P>>(
        &self,
        event_queue: &mut EventQueue<D>,
        version: RangeInclusive<u32>,
    ) -> Result<P, BindError> {
        if !self.is::<P>() {
            return Err(BindError::IncorrectInterface {
                actual: self.interface.to_owned(),
                requested: P::interface().name,
            });
        }

        assert!(*version.end() <= P::interface().version);

        if self.version < *version.start() {
            return Err(BindError::UnsupportedVersion {
                actual: self.version,
                min: *version.start(),
                max: *version.end(),
            });
        }

        let reg = event_queue.registry();
        let version = u32::min(*version.end(), self.version);

        Ok(reg.bind(event_queue, self.name, version))
    }
}

impl GlobalsExt for Globals {
    fn bind<P: Proxy, D: Dispatch<P>>(
        &self,
        event_queue: &mut EventQueue<D>,
        version: RangeInclusive<u32>,
    ) -> Result<P, BindError> {
        let global = self
            .iter()
            .find(|g| g.is::<P>())
            .ok_or(BindError::GlobalNotFound(P::interface().name))?;
        global.bind(event_queue, version)
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
