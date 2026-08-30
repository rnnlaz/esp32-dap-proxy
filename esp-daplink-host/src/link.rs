use std::fmt;
use std::time::Duration;

use socket2::{Socket, TcpKeepalive};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time;

use crate::metrics;

const MAX_FRAME: usize = 4096;

const FRAME_MAGIC: u8 = 0xDA;

#[derive(Debug)]
pub enum LinkError {
    Io(std::io::Error),
    Disconnected,
    Timeout,
    FrameTooLarge(usize),
    Garbage,
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::Io(e) => write!(f, "链路 IO 错误: {e}"),
            LinkError::Disconnected => write!(f, "链路已断开"),
            LinkError::Timeout => write!(f, "链路超时"),
            LinkError::FrameTooLarge(n) => write!(f, "帧长度非法: {n}"),
            LinkError::Garbage => write!(f, "链路数据无法同步"),
        }
    }
}

impl std::error::Error for LinkError {}

pub struct DapLink {
    addr: String,
    stream: Option<TcpStream>,
    read_timeout: Duration,
    write_timeout: Duration,
}

impl DapLink {
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            stream: None,
            // 上限须低于宿主侧 USB 超时
            read_timeout: Duration::from_secs(2),
            write_timeout: Duration::from_secs(1),
        }
    }

    pub fn _addr(&self) -> &str {
        &self.addr
    }

    fn disconnect(&mut self) {
        if self.stream.take().is_some() {
            tracing::debug!("[link:{}] 连接已断开，等待按需重连", self.addr);
        }
    }

    async fn connect(&mut self) -> Result<(), LinkError> {
        let fut = TcpStream::connect(&self.addr);
        let stream = time::timeout(self.write_timeout, fut)
            .await
            .map_err(|_| LinkError::Timeout)?
            .map_err(LinkError::Io)?;
        let _ = stream.set_nodelay(true);

        let std_stream = stream.into_std().map_err(LinkError::Io)?;
        let socket = Socket::from(std_stream);
        let keepalive = TcpKeepalive::new()
            .with_time(Duration::from_secs(10))
            .with_interval(Duration::from_secs(5));
        let _ = socket.set_tcp_keepalive(&keepalive);

        let stream = TcpStream::from_std(socket.into()).map_err(LinkError::Io)?;

        tracing::info!("[link:{}] 已连接 ESP32 DAP 通道", self.addr);
        self.stream = Some(stream);
        Ok(())
    }

    async fn ensure(&mut self) -> Result<(), LinkError> {
        if self.stream.is_none() {
            self.connect().await?;
        }
        Ok(())
    }

    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), LinkError> {
        let stream = self.stream.as_mut().ok_or(LinkError::Disconnected)?;
        let fut = stream.read_exact(buf);
        match time::timeout(self.read_timeout, fut).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                Err(LinkError::Disconnected)
            }
            Ok(Err(e)) => Err(LinkError::Io(e)),
            Err(_) => Err(LinkError::Timeout),
        }
    }

    async fn write_all(&mut self, data: &[u8]) -> Result<(), LinkError> {
        let stream = self.stream.as_mut().ok_or(LinkError::Disconnected)?;
        let fut = stream.write_all(data);
        match time::timeout(self.write_timeout, fut).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(LinkError::Io(e)),
            Err(_) => Err(LinkError::Timeout),
        }
    }

    pub async fn request(&mut self, payload: &[u8]) -> Result<Vec<u8>, LinkError> {
        if payload.len() > MAX_FRAME {
            return Err(LinkError::FrameTooLarge(payload.len()));
        }
        metrics::bump(&metrics::LINK_REQUESTS);
        let t0 = std::time::Instant::now();
        let err = match self.try_request(payload).await {
            Ok(resp) => {
                metrics::note_rtt(t0.elapsed().as_micros() as u64);
                return Ok(resp);
            }
            Err(e) => e,
        };
        tracing::warn!("[link:{}] 请求失败（{err}），重连后重试一次", self.addr);
        metrics::bump(&metrics::LINK_RETRIES);
        self.disconnect();
        self.connect().await?;
        let result = self.try_request(payload).await;
        if let Err(e) = &result {
            metrics::note_link_error(e.to_string());
        }
        result
    }

    async fn try_request(&mut self, payload: &[u8]) -> Result<Vec<u8>, LinkError> {
        self.ensure().await?;

        let mut wire = Vec::with_capacity(3 + payload.len());
        wire.push(FRAME_MAGIC);
        wire.push((payload.len() & 0xFF) as u8);
        wire.push((payload.len() >> 8) as u8);
        wire.extend_from_slice(payload);

        if let Err(e) = self.write_all(&wire).await {
            self.disconnect();
            return Err(e);
        }

        match self.read_frame().await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                self.disconnect();
                Err(e)
            }
        }
    }

    async fn read_frame(&mut self) -> Result<Vec<u8>, LinkError> {
        let mut skipped = 0usize;
        loop {
            let mut byte = [0u8; 1];
            self.read_exact(&mut byte).await?;
            if byte[0] == FRAME_MAGIC {
                break;
            }
            skipped += 1;
            if skipped > MAX_FRAME {
                return Err(LinkError::Garbage);
            }
        }
        let mut len = [0u8; 2];
        self.read_exact(&mut len).await?;
        let n = len[0] as usize | ((len[1] as usize) << 8);
        if n > MAX_FRAME {
            return Err(LinkError::FrameTooLarge(n));
        }
        let mut payload = vec![0u8; n];
        self.read_exact(&mut payload).await?;
        Ok(payload)
    }
}
