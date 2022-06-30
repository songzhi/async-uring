use smol::net::{TcpListener, TcpStream};

#[test]
fn use_smol_types_from_runtime() {
    async_uring::start(async {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let task = async_uring::spawn(async move {
            let _socket = TcpStream::connect(addr).await.unwrap();
        });

        // Accept a connection
        let (_socket, _) = listener.accept().await.unwrap();

        // Wait for the task to complete
        task.await;
    });
}

#[test]
fn spawn_a_task() {
    use std::{cell::RefCell, rc::Rc};

    async_uring::start(async {
        let cell = Rc::new(RefCell::new(1));
        let c = cell.clone();
        let handle = async_uring::spawn(async move {
            *c.borrow_mut() = 2;
        });

        handle.await;
        assert_eq!(2, *cell.borrow());
    });
}
