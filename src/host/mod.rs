pub mod tcp;

use esp_println::println;

use crate::cmd::dap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Tcp,
    Usb,
    Ble,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Accept,
}

pub trait Channel: embedded_io_async::Read + embedded_io_async::Write {
    async fn accept(&mut self) -> Result<(), Error>;

    fn finish(&mut self);
}

pub async fn run<C: Channel>(ch: &mut C) {
    loop {
        match ch.accept().await {
            Ok(()) => {
                println!("[host] Client connected!");
                dap::serve(ch).await;
                println!("[host] Client disconnected!");
            }
            Err(e) => {
                println!("[host] Failed to accept connection: {:?}", e);
            }
        }
        ch.finish();
    }
}
