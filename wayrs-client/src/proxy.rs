use crate::interface::Interface;
use crate::object::{Object, ObjectId};
use crate::wire::Message;

#[derive(Debug)]
pub struct BadMessage;

#[derive(Debug)]
pub struct WrongObject;

pub trait Proxy:
    TryFrom<Object, Error = WrongObject> + Into<Object> + Into<ObjectId> + Copy
{
    type Event;

    fn interface() -> &'static Interface;
    fn null() -> Self;
    fn parse_event(&self, event: Message) -> Result<Self::Event, BadMessage>;
    fn id(&self) -> ObjectId;
    fn version(&self) -> u32;
}
