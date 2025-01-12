use std::any::Any;
use std::collections::VecDeque;
use std::io;
use std::os::fd::{OwnedFd, RawFd};

use wayrs_core::transport::Transport;
use wayrs_core::IoMode;

pub struct AnyTranpsort(Box<dyn AnyTransportImp>);

impl AnyTranpsort {
    pub fn new<T>(transport: T) -> Self
    where
        T: Transport + Send + 'static,
    {
        Self(Box::new(transport))
    }

    pub fn as_any(&self) -> &dyn Any {
        self.0.as_any()
    }

    pub fn as_any_mut(&mut self) -> &mut dyn Any {
        self.0.as_any_mut()
    }
}

trait AnyTransportImp: Transport + Send {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Transport + Send + 'static> AnyTransportImp for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Transport for AnyTranpsort {
    fn pollable_fd(&self) -> RawFd {
        self.0.as_ref().pollable_fd()
    }

    fn send(&mut self, bytes: &[io::IoSlice], fds: &[OwnedFd], mode: IoMode) -> io::Result<usize> {
        self.0.as_mut().send(bytes, fds, mode)
    }

    fn recv(
        &mut self,
        bytes: &mut [io::IoSliceMut],
        fds: &mut VecDeque<OwnedFd>,
        mode: IoMode,
    ) -> io::Result<usize> {
        self.0.as_mut().recv(bytes, fds, mode)
    }
}
