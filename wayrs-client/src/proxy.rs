use crate::object::Object;

use wayrs_core::{Interface, Message, ObjectId};

#[derive(Debug)]
pub struct BadMessage;

#[derive(Debug)]
pub struct WrongObject;

/// A Wayland object proxy.
///
/// This trait is implemented automatically for generated proxies, do not implement it yourself.
pub trait Proxy: TryFrom<Object, Error = WrongObject> + Copy {
    type Event;

    const INTERFACE: &'static Interface;

    #[doc(hidden)]
    fn new(id: ObjectId, version: u32) -> Self;

    #[doc(hidden)]
    fn parse_event(event: Message, version: u32) -> Result<Self::Event, BadMessage>;

    fn id(&self) -> ObjectId;

    fn version(&self) -> u32;
}

impl<P: Proxy> From<P> for Object {
    fn from(value: P) -> Self {
        Self {
            id: value.id(),
            interface: P::INTERFACE,
            version: value.version(),
        }
    }
}
