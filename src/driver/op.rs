use crate::driver;

use io_uring::squeue;
use std::{
    cell::RefCell,
    future::Future,
    io,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

/// In-flight operation
pub(crate) struct Op<T: 'static> {
    // Driver running the operation
    pub(super) driver: Rc<RefCell<driver::Inner>>,

    // Operation index in the slab
    pub(super) index: usize,

    // Per-operation data
    data: Option<T>,
}

/// Operation completion. Returns stored state with the result of the operation.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct Completion<T> {
    pub(crate) data: T,
    pub(crate) result: io::Result<u32>,
    pub(crate) flags: u32,
}

pub(crate) enum Lifecycle {
    /// The operation has been submitted to uring and is currently in-flight
    Submitted,

    /// The submitter is waiting for the completion of the operation
    Waiting(Waker),

    /// The submitter no longer has interest in the operation result. The state
    /// must be passed to the driver and held until the operation completes.
    Ignored(Box<dyn std::any::Any>),

    /// The operation has completed.
    Completed(io::Result<u32>, u32),
}

impl<T> Op<T> {
    /// Create a new operation
    fn new(data: T, inner: &mut driver::Inner, inner_rc: &Rc<RefCell<driver::Inner>>) -> Op<T> {
        Op {
            driver: inner_rc.clone(),
            index: inner.ops.insert(),
            data: Some(data),
        }
    }

    /// Submit an operation to uring.
    ///
    /// `state` is stored during the operation tracking any state submitted to
    /// the kernel.
    pub(super) fn submit_with<F>(data: T, f: F) -> io::Result<Op<T>>
    where
        F: FnOnce(&mut T) -> squeue::Entry,
    {
        driver::CURRENT.with(|inner_rc| {
            let mut inner_ref = inner_rc.borrow_mut();
            let inner = &mut *inner_ref;

            // If the submission queue is full, flush it to the kernel
            if inner.uring.submission().is_full() {
                inner.submit()?;
            }

            // Create the operation
            let mut op = Op::new(data, inner, inner_rc);

            // Configure the SQE
            let sqe = f(op.data.as_mut().unwrap()).user_data(op.index as _);

            {
                let mut sq = inner.uring.submission();

                // Push the new operation
                if unsafe { sq.push(&sqe).is_err() } {
                    unimplemented!("when is this hit?");
                }
            }

            // Submit the new operation. At this point, the operation has been
            // pushed onto the queue and the tail pointer has been updated, so
            // the submission entry is visible to the kernel. If there is an
            // error here (probably EAGAIN), we still return the operation. A
            // future `io_uring_enter` will fully submit the event.
            let _ = inner.submit();
            Ok(op)
        })
    }

    /// Try submitting an operation to uring
    pub(super) fn try_submit_with<F>(data: T, f: F) -> io::Result<Op<T>>
    where
        F: FnOnce(&mut T) -> squeue::Entry,
    {
        if driver::CURRENT.is_set() {
            Op::submit_with(data, f)
        } else {
            Err(io::ErrorKind::Other.into())
        }
    }
}

impl<T> Future for Op<T>
where
    T: Unpin + 'static,
{
    type Output = Completion<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::mem;

        let me = &mut *self;
        let mut inner = me.driver.borrow_mut();
        let lifecycle = inner.ops.get_mut(me.index).expect("invalid internal state");

        match mem::replace(lifecycle, Lifecycle::Submitted) {
            Lifecycle::Submitted => {
                *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                Poll::Pending
            }
            Lifecycle::Waiting(waker) if !waker.will_wake(cx.waker()) => {
                *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                Poll::Pending
            }
            Lifecycle::Waiting(waker) => {
                *lifecycle = Lifecycle::Waiting(waker);
                Poll::Pending
            }
            Lifecycle::Ignored(..) => unreachable!(),
            Lifecycle::Completed(result, flags) => {
                inner.ops.remove(me.index);
                me.index = usize::MAX;

                Poll::Ready(Completion {
                    data: me.data.take().expect("unexpected operation state"),
                    result,
                    flags,
                })
            }
        }
    }
}

impl<T> Drop for Op<T> {
    fn drop(&mut self) {
        let mut inner = self.driver.borrow_mut();
        let lifecycle = match inner.ops.get_mut(self.index) {
            Some(lifecycle) => lifecycle,
            None => return,
        };

        match lifecycle {
            Lifecycle::Submitted | Lifecycle::Waiting(_) => {
                *lifecycle = Lifecycle::Ignored(Box::new(self.data.take()));
            }
            Lifecycle::Completed(..) => {
                inner.ops.remove(self.index);
            }
            Lifecycle::Ignored(..) => unreachable!(),
        }
    }
}

impl Lifecycle {
    pub(super) fn complete(&mut self, result: io::Result<u32>, flags: u32) -> bool {
        use std::mem;

        match mem::replace(self, Lifecycle::Submitted) {
            Lifecycle::Submitted => {
                *self = Lifecycle::Completed(result, flags);
                false
            }
            Lifecycle::Waiting(waker) => {
                *self = Lifecycle::Completed(result, flags);
                waker.wake();
                false
            }
            Lifecycle::Ignored(..) => true,
            Lifecycle::Completed(..) => unreachable!("invalid operation state"),
        }
    }
}
