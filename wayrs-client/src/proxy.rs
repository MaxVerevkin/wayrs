use crate::interface::Interface;
use crate::object::{Object, ObjectId};
use crate::wire::Message;

#[derive(Debug)]
pub struct BadMessage;

#[derive(Debug)]
pub struct WrongObject;

/// A Wayland object proxy.
///
/// This trait is implemented automatically for generated proxies, do not implement it yourself.
pub trait Proxy:
    TryFrom<Object, Error = WrongObject> + Into<Object> + Into<ObjectId> + Copy
{
    type Event;

    const INTERFACE: &'static Interface;

    #[doc(hidden)]
    fn new(id: ObjectId, version: u32) -> Self;

    fn parse_event(&self, event: Message) -> Result<Self::Event, BadMessage>;
    fn id(&self) -> ObjectId;
    fn version(&self) -> u32;

    fn null() -> Self {
        Self::new(ObjectId::NULL, 0)
    }

    fn is_null(&self) -> bool {
        self.id() == ObjectId::NULL
    }
}
