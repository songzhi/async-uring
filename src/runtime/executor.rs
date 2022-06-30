use async_task::{Runnable, Task};
use atomic_waker::AtomicWaker;
use cache_padded::CachePadded;
use concurrent_queue::ConcurrentQueue;
use futures_lite::{
    future::{self, yield_now},
    FutureExt,
};
use scoped_tls::scoped_thread_local;
use std::{
    cell::UnsafeCell,
    collections::VecDeque,
    future::Future,
    marker::PhantomData,
    sync::Arc,
    task::{Poll, Waker},
};

scoped_thread_local!(static CURRENT: LocalExecutor);

pub fn spawn<T: Future + 'static>(task: T) -> Task<T::Output> {
    CURRENT.with(|exec| exec.spawn(task))
}

mod thread_id {
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub fn create() -> usize {
        static NEXT_THREAD_ID: AtomicUsize = AtomicUsize::new(0);

        NEXT_THREAD_ID.fetch_add(1, Ordering::Acquire)
    }

    #[inline]
    pub fn current() -> Option<usize> {
        super::CURRENT
            .is_set()
            .then(|| super::CURRENT.with(|exec| exec.thread_id))
    }
}

#[derive(Debug)]
struct State {
    global_queue: ConcurrentQueue<Runnable>,
    sleep_waker: AtomicWaker,
    local_state: CachePadded<UnsafeCell<LocalState>>,
}

#[derive(Debug)]
struct LocalState {
    ticks: usize,
    queue: VecDeque<Runnable>,
}

impl State {
    fn new() -> Self {
        Self {
            global_queue: ConcurrentQueue::unbounded(),
            sleep_waker: AtomicWaker::new(),
            local_state: CachePadded::new(UnsafeCell::new(LocalState {
                ticks: 0,
                queue: VecDeque::new(),
            })),
        }
    }

    fn push_local(&self, runnable: Runnable) {
        let local_state = unsafe { &mut *self.local_state.get() };
        local_state.queue.push_back(runnable);
        self.wake();
    }

    fn push_global(&self, runnable: Runnable) {
        self.global_queue.push(runnable).unwrap();
        self.wake();
    }

    fn wake(&self) {
        if let Some(waker) = self.sleep_waker.take() {
            waker.wake();
        }
    }

    fn sleep(&self, waker: &Waker) {
        self.sleep_waker.register(waker);
    }

    async fn runnable_with(&self, mut search: impl FnMut() -> Option<Runnable>) -> Runnable {
        future::poll_fn(|cx| match search() {
            None => {
                self.sleep(cx.waker());
                Poll::Pending
            }
            Some(r) => {
                let local_state = unsafe { &mut *self.local_state.get() };
                local_state.ticks += 1;

                Poll::Ready(r)
            }
        })
        .await
    }

    async fn runnable(&self) -> Runnable {
        self.runnable_with(|| {
            let local_state = unsafe { &mut *self.local_state.get() };
            if local_state.ticks % 50 == 0 || local_state.queue.is_empty() {
                let runnable = self
                    .global_queue
                    .pop()
                    .ok()
                    .or_else(|| local_state.queue.pop_front())?;

                while let Ok(runnable) = self.global_queue.pop() {
                    local_state.queue.push_back(runnable);
                }
                Some(runnable)
            } else {
                local_state.queue.pop_front()
            }
        })
        .await
    }

    fn schedule(&self, runnable: Runnable, target_thread_id: usize) {
        if thread_id::current() == Some(target_thread_id) {
            self.push_local(runnable);
        } else {
            self.push_global(runnable);
        }
    }
}

pub struct LocalExecutor {
    state: Arc<State>,
    thread_id: usize,
    // Make sure the type is `!Send` and `!Sync`.
    _marker: PhantomData<*const ()>,
}

impl LocalExecutor {
    pub fn new() -> Self {
        Self {
            state: Arc::new(State::new()),
            thread_id: thread_id::create(),
            _marker: PhantomData,
        }
    }

    pub fn spawn<T>(&self, future: impl Future<Output = T>) -> Task<T> {
        let (runnable, task) = unsafe { async_task::spawn_unchecked(future, self.schedule_fn()) };
        self.state.push_local(runnable);
        task
    }

    pub fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        CURRENT.set(self, f)
    }

    pub async fn run<T>(&self, future: impl Future<Output = T>) -> T {
        assert!(CURRENT.is_set());
        let run_forever = async {
            loop {
                for _ in 0..100 {
                    self.state.runnable().await.run();
                }
                yield_now().await;
            }
        };

        future.or(run_forever).await
    }

    fn schedule_fn(&self) -> impl Fn(Runnable) + 'static {
        let state = self.state.clone();
        let thread_id = self.thread_id;

        move |runnable| {
            state.schedule(runnable, thread_id);
        }
    }
}
