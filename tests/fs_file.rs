use async_uring::fs::File;
use futures_lite::future::poll_fn;
use std::{
    future::Future,
    io::prelude::*,
    os::unix::io::{AsRawFd, FromRawFd, RawFd},
};
use tempfile::NamedTempFile;

const HELLO: &[u8] = b"hello world...";

async fn read_hello(file: &File) {
    let buf = Vec::with_capacity(1024);
    let (res, buf) = file.read_at(buf, 0).await;
    let n = res.unwrap();

    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

#[test]
fn basic_read() {
    async_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::open(tempfile.path()).await.unwrap();

        read_hello(&file).await;
    });
}

#[test]
fn basic_read_exact() {
    async_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::open(tempfile.path()).await.unwrap();
        let buf = Vec::with_capacity(HELLO.len());
        let (res, buf) = file.read_exact_at(buf, 0).await;
        res.unwrap();
        assert_eq!(&buf[..], HELLO);

        let buf = Vec::with_capacity(HELLO.len() * 2);
        let (res, _) = file.read_exact_at(buf, 0).await;
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::UnexpectedEof);
    });
}

#[test]
fn basic_write() {
    async_uring::start(async {
        let tempfile = tempfile();

        let file = File::create(tempfile.path()).await.unwrap();

        file.write_at(HELLO, 0).await.0.unwrap();

        let file = std::fs::read(tempfile.path()).unwrap();
        assert_eq!(file, HELLO);
    });
}

#[test]
fn basic_write_all() {
    async_uring::start(async {
        let tempfile = tempfile();

        let file = File::create(tempfile.path()).await.unwrap();
        file.write_all_at(HELLO, 0).await.0.unwrap();

        let file = std::fs::read(tempfile.path()).unwrap();
        assert_eq!(file, HELLO);
    });
}

#[test]
fn cancel_read() {
    async_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::open(tempfile.path()).await.unwrap();

        // Poll the future once, then cancel it
        poll_once(async { read_hello(&file).await }).await;

        read_hello(&file).await;
    });
}

// #[test]
// fn explicit_close() {
//     let mut tempfile = tempfile();
//     tempfile.write_all(HELLO).unwrap();

//     async_uring::start(async {
//         let file = File::open(tempfile.path()).await.unwrap();
//         let fd = file.as_raw_fd();
//
//         file.close().await.unwrap();

//         assert_invalid_fd(fd);
//     })
// }

#[test]
fn drop_open() {
    async_uring::start(async {
        let tempfile = tempfile();
        let _ = File::create(tempfile.path());

        // Do something else
        let file = File::create(tempfile.path()).await.unwrap();

        file.write_at(HELLO, 0).await.0.unwrap();

        let file = std::fs::read(tempfile.path()).unwrap();
        assert_eq!(file, HELLO);
    });
}

#[test]
fn drop_off_runtime() {
    let file = async_uring::start(async {
        let tempfile = tempfile();
        File::open(tempfile.path()).await.unwrap()
    });

    let fd = file.as_raw_fd();

    drop(file);

    assert_invalid_fd(fd);
}

#[test]
fn sync_doesnt_kill_anything() {
    let tempfile = tempfile();

    async_uring::start(async {
        let file = File::create(tempfile.path()).await.unwrap();
        file.sync_all().await.unwrap();
        file.sync_data().await.unwrap();
        file.write_at(&b"foo"[..], 0).await.0.unwrap();
        file.sync_all().await.unwrap();
        file.sync_data().await.unwrap();
    });
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}

async fn poll_once(future: impl Future) {
    use std::task::Poll;

    futures_lite::pin!(future);

    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

fn assert_invalid_fd(fd: RawFd) {
    use std::fs::File;

    let mut f = unsafe { File::from_raw_fd(fd) };
    let mut buf = vec![];

    match f.read_to_end(&mut buf) {
        Err(ref e) if e.raw_os_error() == Some(libc::EBADF) => {}
        res => panic!("{:?}", res),
    }
}
