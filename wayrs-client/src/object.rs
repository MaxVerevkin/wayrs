use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};
use std::num::NonZeroU32;

use crate::connection::GenericCallback;
use crate::interface::Interface;
use crate::protocol::WlDisplay;
use crate::proxy::Proxy;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub NonZeroU32);

impl ObjectId {
    pub const DISPLAY: Self = Self(unsafe { NonZeroU32::new_unchecked(1) });

    pub const MAX_CLIENT: Self = Self(unsafe { NonZeroU32::new_unchecked(0xFEFFFFFF) });

    pub fn next(self) -> Self {
        Self(self.0.checked_add(1).expect("ObjectId overflow"))
    }

    pub fn created_by_server(self) -> bool {
        self > Self::MAX_CLIENT
    }
}

pub(crate) struct ObjectManager<D> {
    max_used_id: ObjectId,
    vacant_ids: Vec<ObjectId>,
    objects: HashMap<ObjectId, ObjectState<D>>,
    dead_objects: HashMap<ObjectId, Object>,
    pub display: WlDisplay,
}

pub(crate) struct ObjectState<D> {
    pub object: Object,
    pub cb: Option<GenericCallback<D>>,
}

impl<D> ObjectManager<D> {
    pub fn new() -> Self {
        let mut this = Self {
            max_used_id: ObjectId::DISPLAY,
            vacant_ids: Vec::new(),
            objects: HashMap::new(),
            dead_objects: HashMap::new(),
            display: WlDisplay::new(ObjectId::DISPLAY, 1),
        };
        let _ = this.alloc_with_id(ObjectId::DISPLAY, WlDisplay::INTERFACE, 1);
        this
    }

    pub fn alloc(&mut self, interface: &'static Interface, version: u32) -> &mut ObjectState<D> {
        let id = self.vacant_ids.pop().unwrap_or_else(|| {
            let id = self.max_used_id.next();
            assert!(!id.created_by_server());
            self.max_used_id = id;
            id
        });
        self.alloc_with_id(id, interface, version)
    }

    pub fn alloc_with_id(
        &mut self,
        id: ObjectId,
        interface: &'static Interface,
        version: u32,
    ) -> &mut ObjectState<D> {
        self.objects.entry(id).or_insert(ObjectState {
            object: Object {
                id,
                interface,
                version,
            },
            cb: None,
        })
    }

    pub fn get_object(&self, id: ObjectId) -> Option<Object> {
        self.objects.get(&id).map(|o| o.object)
    }

    pub fn get_object_live_or_dead(&self, id: ObjectId) -> Option<Object> {
        self.get_object(id)
            .or_else(|| self.dead_objects.get(&id).copied())
    }

    pub fn get_object_state_mut(&mut self, id: ObjectId) -> Option<&mut ObjectState<D>> {
        self.objects.get_mut(&id)
    }

    /// Destroy the object.
    ///
    /// Due to the async-ness of Wayland, the object may still receive events after being destroyed
    /// by the client. After this call the object is considered "dead" or "zombie". You cannot send
    /// requests to this object and all received events are ignored.
    pub fn destroy(&mut self, id: ObjectId) {
        if let Some(obj) = self.objects.remove(&id) {
            self.dead_objects.insert(id, obj.object);
        }
    }

    /// Delete the object.
    ///
    /// The difference between this method and [`destroy`] is that this method does not make the
    /// object "dead" but instead completely deletes it. Call it only on client-created objects in
    /// response to `wl_display.delete_id`.
    pub fn delete(&mut self, id: ObjectId) {
        assert!(!id.created_by_server());
        self.objects.remove(&id);
        self.dead_objects.remove(&id);
        self.vacant_ids.push(id);
    }
}
