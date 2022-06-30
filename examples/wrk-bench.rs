use async_task::Task;
use std::io;
use std::rc::Rc;

pub const RESPONSE: &[u8] =
    b"HTTP/1.1 200 OK\nContent-Type: text/plain\nContent-Length: 12\n\nHello world!\n";

pub const ADDRESS: &str = "127.0.0.1:8080";

fn main() -> io::Result<()> {
    async_uring::start(async {
        let mut tasks = Vec::with_capacity(16);
        let listener = Rc::new(async_uring::net::TcpListener::bind(
            ADDRESS.parse().unwrap(),
        )?);

        for _ in 0..16 {
            let listener = listener.clone();
            let task: Task<io::Result<()>> = async_uring::spawn(async move {
                loop {
                    let (stream, _) = listener.accept().await?;

                    async_uring::spawn(async move {
                        let mut buf = vec![0; 128];
                        loop {
                            let (res, r_buf) = stream.read(buf).await;
                            buf = r_buf;
                            if res.is_err() {
                                break;
                            };
                            let (result, _) = stream.write(RESPONSE).await;

                            if result.is_err() {
                                break;
                            }
                        }
                    })
                    .detach();
                }
            });
            tasks.push(task);
        }

        for t in tasks {
            t.await.unwrap();
        }

        Ok(())
    })
}
