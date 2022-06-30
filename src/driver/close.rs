use crate::driver::Op;
use std::{io, os::unix::io::RawFd};

pub(crate) struct Close {
    fd: RawFd,
}

impl Op<Close> {
    pub(crate) fn close(fd: RawFd) -> io::Result<Op<Close>> {
        use io_uring::{opcode, types};

        Op::try_submit_with(Close { fd }, |close| {
            opcode::Close::new(types::Fd(close.fd)).build()
        })
    }
}
