use embassy_net::{
    IpListenEndpoint, Stack, tcp::TcpSocket,
};
use embedded_io_async::{ErrorType, Read, Write};

use super::{Channel, Error};

pub struct TcpChannel<'a> {
    socket: TcpSocket<'a>,
    port: u16,
}

impl<'a> TcpChannel<'a> {
    pub fn new(stack: Stack<'a>, port: u16, rx_buffer: &'a mut [u8], tx_buffer: &'a mut [u8]) -> Self {
        let mut socket = TcpSocket::new(stack, rx_buffer, tx_buffer);
        socket.set_timeout(None);
        Self { socket, port }
    }
}

impl Channel for TcpChannel<'_> {
    async fn accept(&mut self) -> Result<(), Error> {
        self.socket
            .accept(IpListenEndpoint {
                addr: None,
                port: self.port,
            }).await
            .map_err(|_| Error::Accept)
    }

    fn finish(&mut self) {
        self.socket.abort();
    }
}

impl ErrorType for TcpChannel<'_> {
    type Error = embassy_net::tcp::Error;
}

impl Read for TcpChannel<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.socket.read(buf).await
    }
}

impl Write for TcpChannel<'_> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.socket.write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.socket.flush().await
    }
}