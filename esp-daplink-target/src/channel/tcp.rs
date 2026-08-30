use embassy_net::{IpListenEndpoint, Stack, tcp::TcpSocket};
use embedded_io_async::Write;

use super::frame::{Deframer, FRAME_MAGIC};
use super::{Channel, Error};

pub struct TcpChannel<'a> {
    socket: TcpSocket<'a>,
    port: u16,
    deframer: Deframer<1024>,
}

impl<'a> TcpChannel<'a> {
    pub fn new(
        stack: Stack<'a>,
        port: u16,
        rx_buffer: &'a mut [u8],
        tx_buffer: &'a mut [u8],
    ) -> Self {
        let mut socket = TcpSocket::new(stack, rx_buffer, tx_buffer);
        socket.set_timeout(None);
        Self {
            socket,
            port,
            deframer: Deframer::new(),
        }
    }
}

impl Channel for TcpChannel<'_> {
    async fn accept(&mut self) -> Result<(), Error> {
        self.socket
            .accept(IpListenEndpoint {
                addr: None,
                port: self.port,
            })
            .await
            .map_err(|_| Error::Accept)
    }

    async fn recv_frame(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let mut chunk = [0u8; 128];
        loop {
            let n = self
                .socket
                .read(&mut chunk)
                .await
                .map_err(|_| Error::Disconnected)?;
            if n == 0 {
                return Err(Error::Disconnected);
            }
            for &b in &chunk[..n] {
                if let Some(frame) = self.deframer.feed(b) {
                    if frame.len() > buf.len() {
                        return Err(Error::FrameTooLarge);
                    }
                    buf[..frame.len()].copy_from_slice(frame);
                    return Ok(frame.len());
                }
            }
            if self.deframer.is_garbage() {
                return Err(Error::Garbage);
            }
        }
    }

    async fn send_frame(&mut self, buf: &[u8]) -> Result<(), Error> {
        let head = [
            FRAME_MAGIC,
            (buf.len() & 0xFF) as u8,
            (buf.len() >> 8) as u8,
        ];
        self.socket
            .write_all(&head)
            .await
            .map_err(|_| Error::Disconnected)?;
        self.socket
            .write_all(buf)
            .await
            .map_err(|_| Error::Disconnected)?;
        self.socket.flush().await.map_err(|_| Error::Disconnected)
    }

    fn finish(&mut self) {
        self.socket.abort();
    }
}
