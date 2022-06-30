use crate::driver::Op;
use std::{io, os::unix::io::RawFd};

pub(crate) struct Fadvise {
    fd: RawFd,
}

impl Op<Fadvise> {
    pub(crate) fn fadvise(fd: RawFd, len: libc::off_t, advice: i32) -> io::Result<Self> {
        use io_uring::{opcode, types};

        Op::try_submit_with(Fadvise { fd }, |fadvise| {
            opcode::Fadvise::new(types::Fd(fadvise.fd), len, advice).build()
        })
    }
}
