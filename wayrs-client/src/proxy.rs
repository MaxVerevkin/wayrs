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
    type Event: TryFrom<Message, Error = BadMessage>;

    const INTERFACE: &'static Interface;

    #[doc(hidden)]
    fn new(id: ObjectId, version: u32) -> Self;

    fn id(&self) -> ObjectId;

    fn version(&self) -> u32;
}
