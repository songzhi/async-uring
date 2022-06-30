use self::executor::LocalExecutor;
use crate::driver::Driver;
use futures_lite::FutureExt;
use std::{future::Future, io};

mod executor;

pub use self::executor::spawn;

pub struct Runtime {
    driver: async_io::Async<Driver>,
    executor: LocalExecutor,
}

impl Runtime {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            driver: async_io::Async::new(Driver::new()?)?,
            executor: LocalExecutor::new(),
        })
    }

    pub fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        self.executor.with(|| {
            self.driver.get_ref().with(|| {
                let drive = async {
                    loop {
                        self.driver.readable().await.unwrap();
                        self.driver.as_ref().tick();
                    }
                };

                pin!(drive);
                pin!(future);

                async_io::block_on(self.executor.run(drive.or(future)))
            })
        })
    }
}
