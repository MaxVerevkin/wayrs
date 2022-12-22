use crate::event_queue::EventQueue;
use crate::interface::Interface;
use crate::object::{Object, ObjectId};
use crate::wire::Message;

#[derive(Debug)]
pub struct BadMessage;

#[derive(Debug)]
pub struct WrongObject;

pub trait Proxy: TryFrom<Object, Error = WrongObject> {
    type Event: TryFrom<Message, Error = BadMessage>;

    fn interface() -> &'static Interface;
    fn id(&self) -> ObjectId;
    fn version(&self) -> u32;
}

pub trait Dispatcher {
    type Error;
}

pub trait Dispatch<P: Proxy>: Dispatcher + Sized {
    fn event(&mut self, event_queue: &mut EventQueue<Self>, proxy: P, event: P::Event) {
        let _ = (event_queue, proxy, event);
    }

    fn try_event(
        &mut self,
        event_queue: &mut EventQueue<Self>,
        proxy: P,
        event: P::Event,
    ) -> Result<(), Self::Error> {
        self.event(event_queue, proxy, event);
        Ok(())
    }
}

pub type EventCallback<D> =
    fn(&mut EventQueue<D>, &mut D, Object, Message) -> Result<(), <D as Dispatcher>::Error>;

pub(crate) fn make_callback<P: Proxy, D: Dispatch<P>>(
    event_queue: &mut EventQueue<D>,
    state: &mut D,
    obj: Object,
    event: Message,
) -> Result<(), D::Error> {
    state.try_event(
        event_queue,
        obj.try_into().unwrap(),
        event.try_into().unwrap(),
    )
}
