use crate::driver::{Op, SharedFd};
use std::{boxed::Box, io};

pub(crate) struct Accept {
    fd: SharedFd,
    pub(crate) socket_addr: Box<(libc::sockaddr_storage, libc::socklen_t)>,
}

impl Op<Accept> {
    pub(crate) fn accept(fd: &SharedFd) -> io::Result<Op<Accept>> {
        use io_uring::{opcode, types};

        let socket_addr = Box::new((
            unsafe { std::mem::zeroed() },
            std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t,
        ));
        Op::submit_with(
            Accept {
                fd: fd.clone(),
                socket_addr,
            },
            |accept| {
                opcode::Accept::new(
                    types::Fd(accept.fd.raw_fd()),
                    &mut accept.socket_addr.0 as *mut _ as *mut _,
                    &mut accept.socket_addr.1,
                )
                .flags(libc::O_CLOEXEC)
                .build()
            },
        )
    }
}
