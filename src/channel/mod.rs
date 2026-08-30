pub mod frame;
pub mod tcp;

use esp_println::println;

use crate::cmd::dap;
use crate::probe::transport::Transport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Accept,
    Disconnected,
    Garbage,
    FrameTooLarge,
}

pub trait Channel {
    async fn accept(&mut self) -> Result<(), Error>;

    async fn recv_frame(&mut self, buf: &mut [u8]) -> Result<usize, Error>;

    async fn send_frame(&mut self, buf: &[u8]) -> Result<(), Error>;

    fn finish(&mut self);
}

pub async fn run<C: Channel, T: Transport>(ch: &mut C, transport: &mut T) {
    loop {
        match ch.accept().await {
            Ok(()) => {
                println!("[host] Client connected!");
                dap::serve(ch, transport).await;
                println!("[host] Client disconnected!");
            }
            Err(e) => {
                println!("[host] Failed to accept connection: {:?}", e);
            }
        }
        ch.finish();
    }
}
