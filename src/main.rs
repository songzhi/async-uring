use async_io::Timer;
use async_uring::*;
use std::time::Duration;

fn main() {
    start(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("Hello");
    });
}
