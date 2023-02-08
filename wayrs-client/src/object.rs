use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};

use crate::interface::Interface;

/// A Wayland object.
///
/// The [`Debug`] representation is `<interface>@<id>v<version>`.
#[derive(Clone, Copy)]
pub struct Object {
    pub id: ObjectId,
    pub interface: &'static Interface,
    pub version: u32,
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Object {}

impl Hash for Object {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}@{}v{}",
            self.interface.name.to_string_lossy(),
            self.id.0,
            self.version
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId(pub u32);

impl ObjectId {
    pub const NULL: Self = Self(0);
    pub const DISPLAY: Self = Self(1);

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    pub fn created_by_server(self) -> bool {
        self.0 >= 0xFF000000
    }
}
