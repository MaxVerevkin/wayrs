//! Wayland transport over unix domain socket
//!
//! This is the most commonly used Wayland transport method.

use std::collections::VecDeque;
use std::io::{self, IoSlice, IoSliceMut};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

use super::{Transport, FDS_IN_LEN, FDS_OUT_LEN};
use crate::IoMode;

impl Transport for UnixStream {
    fn pollable_fd(&self) -> RawFd {
        self.as_raw_fd()
    }

    fn send(&mut self, bytes: &[IoSlice], fds: &[OwnedFd], mode: IoMode) -> io::Result<usize> {
        let mut flags = libc::MSG_NOSIGNAL;
        if mode == IoMode::NonBlocking {
            flags |= libc::MSG_DONTWAIT;
        }

        let mut cmsg = [0u8; cmsg_space(std::mem::size_of::<[OwnedFd; FDS_OUT_LEN]>())];

        let mhdr = {
            let mut mhdr = unsafe { std::mem::zeroed::<libc::msghdr>() };
            mhdr.msg_iov = bytes.as_ptr().cast_mut().cast();
            mhdr.msg_iovlen = bytes.len() as _;

            if !fds.is_empty() {
                let fds_size = std::mem::size_of_val(fds);
                let controllen = cmsg_space(fds_size);
                assert!(controllen <= cmsg.len());

                mhdr.msg_control = cmsg.as_mut_ptr().cast();
                mhdr.msg_controllen = controllen as _;

                let pmhdr = unsafe { libc::CMSG_FIRSTHDR(&mhdr).as_mut().unwrap() };
                pmhdr.cmsg_level = libc::SOL_SOCKET;
                pmhdr.cmsg_type = libc::SCM_RIGHTS;
                pmhdr.cmsg_len = unsafe { libc::CMSG_LEN(fds_size as libc::c_uint) } as _;
                let dst_ptr = unsafe { libc::CMSG_DATA(pmhdr) };
                let src_ptr = fds.as_ptr().cast();
                unsafe { std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, fds_size) };
            }

            mhdr
        };

        let ret = unsafe { libc::sendmsg(self.as_raw_fd(), &mhdr, flags) };
        if ret == -1 {
            return Err(io::Error::last_os_error());
        }

        Ok(ret as usize)
    }

    fn recv(
        &mut self,
        bytes: &mut [IoSliceMut],
        fds: &mut VecDeque<OwnedFd>,
        mode: IoMode,
    ) -> io::Result<usize> {
        let mut cmsg = [0u8; cmsg_space(std::mem::size_of::<[RawFd; FDS_IN_LEN]>())];

        let mut flags = libc::MSG_CMSG_CLOEXEC | libc::MSG_NOSIGNAL;
        if mode == IoMode::NonBlocking {
            flags |= libc::MSG_DONTWAIT;
        }

        let (read, mut cmsghdr, mhdr) = {
            let (msg_control, msg_controllen) = (cmsg.as_mut_ptr(), cmsg.len());
            let mut mhdr = {
                let mut mhdr = unsafe { std::mem::zeroed::<libc::msghdr>() };
                mhdr.msg_iov = bytes.as_mut_ptr().cast();
                mhdr.msg_iovlen = bytes.len() as _;
                mhdr.msg_control = msg_control.cast();
                mhdr.msg_controllen = msg_controllen as _;
                mhdr
            };

            let ret = unsafe { libc::recvmsg(self.as_raw_fd(), &mut mhdr, flags) };
            if ret == -1 {
                return Err(io::Error::last_os_error());
            }

            // The cast is not unnecessary on all platforms.
            #[allow(clippy::unnecessary_cast)]
            let cmsghdr = {
                let ptr = if mhdr.msg_controllen > 0 {
                    assert!(!mhdr.msg_control.is_null());
                    assert!(msg_controllen >= mhdr.msg_controllen as usize);
                    unsafe { libc::CMSG_FIRSTHDR(&mhdr) }
                } else {
                    std::ptr::null()
                };
                unsafe { ptr.as_ref() }
            };

            (ret as usize, cmsghdr, mhdr)
        };

        while let Some(hdr) = cmsghdr {
            let p = unsafe { libc::CMSG_DATA(hdr) };
            // The cast is not unnecessary on all platforms.
            #[allow(clippy::unnecessary_cast)]
            let len = hdr as *const _ as usize + hdr.cmsg_len as usize - p as usize;
            if hdr.cmsg_level == libc::SOL_SOCKET && hdr.cmsg_type == libc::SCM_RIGHTS {
                let n = len / std::mem::size_of::<RawFd>();
                let p = p.cast::<RawFd>();
                for i in 0..n {
                    let fd = unsafe { p.add(i).read_unaligned() };
                    assert_ne!(fd, -1);
                    fds.push_back(unsafe { OwnedFd::from_raw_fd(fd) });
                }
            }
            cmsghdr = unsafe { libc::CMSG_NXTHDR(&mhdr, hdr).as_ref() };
        }

        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "server disconnected",
            ));
        }

        Ok(read)
    }
}

const fn cmsg_space(len: usize) -> usize {
    unsafe { libc::CMSG_SPACE(len as libc::c_uint) as usize }
}
