use std::borrow::Borrow;
use std::cmp;
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};
use std::num::NonZeroU32;

use crate::connection::GenericCallback;
use crate::interface::Interface;
use crate::protocol::WlDisplay;

/// A Wayland object.
///
/// The [`Debug`] representation is `<interface>@<id>v<version>`.
///
/// [`Eq`], [`Ord`] and [`Hash`] implementations are delegated to the object's ID for performance
/// reasons. This is fine because two different objects with the same ID must not exist at the same
/// time.
#[derive(Clone, Copy)]
pub struct Object {
    pub id: ObjectId,
    pub interface: &'static Interface,
    pub version: u32,
}

impl PartialEq for Object {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Object {}

impl PartialEq<ObjectId> for Object {
    #[inline]
    fn eq(&self, other: &ObjectId) -> bool {
        self.id == *other
    }
}

impl PartialEq<Object> for ObjectId {
    #[inline]
    fn eq(&self, other: &Object) -> bool {
        *self == other.id
    }
}

impl PartialOrd for Object {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Object {
    #[inline]
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl Hash for Object {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Borrow<ObjectId> for Object {
    fn borrow(&self) -> &ObjectId {
        &self.id
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

/// A Wayland object ID.
///
/// Uniquely identifies an object at each point of time. Note that an ID may have a limited
/// lifetime. Also an ID which once pointed to a certain object, may point to a different object in
/// the future, due to ID reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub(crate) NonZeroU32);

impl ObjectId {
    pub(crate) const DISPLAY: Self = Self(unsafe { NonZeroU32::new_unchecked(1) });
    pub(crate) const MAX_CLIENT: Self = Self(unsafe { NonZeroU32::new_unchecked(0xFEFFFFFF) });
    pub(crate) const MIN_SERVER: Self = Self(unsafe { NonZeroU32::new_unchecked(0xFF000000) });

    /// Returns the numeric representation of the ID
    pub fn as_u32(self) -> u32 {
        self.0.get()
    }

    /// Whether the object with this ID was created by the server
    pub fn created_by_server(self) -> bool {
        self >= Self::MIN_SERVER
    }

    /// Whether the object with this ID was created by the client
    pub fn created_by_client(self) -> bool {
        self <= Self::MAX_CLIENT
    }
}

pub(crate) struct ObjectManager<D> {
    vacant_ids: Vec<ObjectId>,
    client_objects: Vec<Option<ObjectState<D>>>,
    server_objects: Vec<Option<ObjectState<D>>>,
}

pub(crate) struct ObjectState<D> {
    pub object: Object,
    pub is_alive: bool,
    pub cb: Option<GenericCallback<D>>,
}

impl<D> ObjectManager<D> {
    pub fn new() -> Self {
        let mut this = Self {
            vacant_ids: Vec::new(),
            client_objects: Vec::with_capacity(16),
            server_objects: Vec::new(),
        };

        // Dummy NULL object
        this.client_objects.push(None);

        // Display
        this.client_objects.push(Some(ObjectState {
            object: WlDisplay::INSTANCE.into(),
            is_alive: true,
            cb: None,
        }));

        this
    }

    pub fn alloc_client_object(
        &mut self,
        interface: &'static Interface,
        version: u32,
    ) -> &mut ObjectState<D> {
        let id = self.vacant_ids.pop().unwrap_or_else(|| {
            let id = self.client_objects.len() as u32;
            self.client_objects.push(None);
            ObjectId(NonZeroU32::new(id).unwrap())
        });

        assert!(id.created_by_client());
        let obj = self.client_objects.get_mut(id.0.get() as usize).unwrap();
        assert!(obj.is_none());

        obj.insert(ObjectState {
            object: Object {
                id,
                interface,
                version,
            },
            is_alive: true,
            cb: None,
        })
    }

    pub fn register_server_object(&mut self, object: Object) -> &mut ObjectState<D> {
        assert!(object.id.created_by_server());

        let index = (object.id.as_u32() - ObjectId::MIN_SERVER.as_u32()) as usize;

        while index >= self.server_objects.len() {
            self.server_objects.push(None);
        }

        self.server_objects[index].insert(ObjectState {
            object,
            is_alive: true,
            cb: None,
        })
    }

    pub fn get_object_mut(&mut self, id: ObjectId) -> Option<&mut ObjectState<D>> {
        if id.created_by_client() {
            self.client_objects
                .get_mut(id.as_u32() as usize)
                .and_then(Option::as_mut)
        } else {
            self.server_objects
                .get_mut((id.as_u32() - ObjectId::MIN_SERVER.as_u32()) as usize)
                .and_then(Option::as_mut)
        }
    }

    /// Call it only on client-created objects in response to `wl_display.delete_id`.
    pub fn delete_client_object(&mut self, id: ObjectId) {
        assert!(id.created_by_client());
        *self.client_objects.get_mut(id.as_u32() as usize).unwrap() = None;
        self.vacant_ids.push(id);
    }
}
