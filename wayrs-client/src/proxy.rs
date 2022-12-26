use crate::connection::Connection;
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

pub trait Dispatcher {
    type Error;
}

pub trait Dispatch<P: Proxy>: Dispatcher + Sized {
    fn event(&mut self, conn: &mut Connection<Self>, proxy: P, event: P::Event) {
        let _ = (conn, proxy, event);
    }

    fn try_event(
        &mut self,
        conn: &mut Connection<Self>,
        proxy: P,
        event: P::Event,
    ) -> Result<(), Self::Error> {
        self.event(conn, proxy, event);
        Ok(())
    }
}

pub type EventCallback<D> =
    fn(&mut Connection<D>, &mut D, Object, Message) -> Result<(), <D as Dispatcher>::Error>;

pub(crate) fn make_callback<P: Proxy, D: Dispatch<P>>(
    conn: &mut Connection<D>,
    state: &mut D,
    obj: Object,
    event: Message,
) -> Result<(), D::Error> {
    let proxy: P = obj.try_into().unwrap();
    let event = proxy.parse_event(event).unwrap();
    state.try_event(conn, proxy, event)
}
