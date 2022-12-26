use std::ffi::CStr;
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};

use crate::wire::ArgType;

pub struct Interface {
    pub name: &'static CStr,
    pub version: u32,
    pub events: &'static [MessageDesc],
    pub requests: &'static [MessageDesc],
}

#[derive(Debug, Clone, Copy)]
pub struct MessageDesc {
    pub name: &'static str,
    pub is_destructor: bool,
    pub signature: &'static [ArgType],
}

impl PartialEq for &'static Interface {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for &'static Interface {}

impl Hash for &'static Interface {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Debug for Interface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Interface").field(&self.name).finish()
    }
}
