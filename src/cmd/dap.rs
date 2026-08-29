use embedded_io_async::{Read, Write};
use esp_println::println;

const FRAME_MAGIC: u8 = 0xDA;
const MAX_REJECTS: usize = 128;

enum RxState {
    Magic,
    LenLo,
    LenHi { lo: u8 },
    Payload { left: usize },
}

pub struct Deframer<const N: usize> {
    buf: [u8; N],
    fill: usize,
    state: RxState,
    rejects: usize,
}

impl<const N: usize> Deframer<N> {
    pub const fn new() -> Self {
        Self { buf: [0; N], fill: 0, state: RxState::Magic, rejects: 0 }
    }

    pub fn reset(&mut self) {
        self.fill = 0;
        self.rejects = 0;
        self.state = RxState::Magic;
    }

    pub fn is_garbage(&self) -> bool {
        self.rejects > MAX_REJECTS
    }

    pub fn feed(&mut self, byte: u8) -> Option<&[u8]> {
        match self.state {
            RxState::Magic => {
                if byte == FRAME_MAGIC {
                    self.state = RxState::LenLo;
                } else {
                    self.rejects += 1;
                }
                None
            }
            RxState::LenLo => {
                self.state = RxState::LenHi { lo: byte };
                None
            }
            RxState::LenHi { lo } => {
                let len = (lo as usize) | ((byte as usize) << 8);
                self.fill = 0;
                if len == 0 {
                    self.state = RxState::Magic;
                } else {
                    self.state = RxState::Payload { left: len };
                }
                None
            }
            RxState::Payload { left } => {
                if self.fill < N {
                    self.buf[self.fill] = byte;
                }
                self.fill += 1;
                if left == 1 {
                    let n = self.fill.min(N);
                    self.state = RxState::Magic;
                    self.rejects = 0; // 正常出帧，清零
                    Some(&self.buf[..n])
                } else {
                    self.state = RxState::Payload { left: left - 1 };
                    None
                }
            }
        }
    }
}

pub async fn serve<I: Read + Write>(io: &mut I) {
    let mut deframer = Deframer::<1024>::new();
    let mut chunk = [0u8; 128];
    let mut resp = [0u8; 1024];

    loop {
        let n = match io.read(&mut chunk).await {
            Ok(0) => return,
            Ok(n) => n,
            Err(_) => return,
        };

        for &b in &chunk[..n] {
            if let Some(frame) = deframer.feed(b) {
                let rn = handle(frame, &mut resp);

                let head = [FRAME_MAGIC, (rn & 0xFF) as u8, (rn >> 8) as u8];
                if io.write_all(&head).await.is_err() { return; }
                if io.write_all(&resp[..rn]).await.is_err() { return; }
                let _ = io.flush().await;
            }
            if deframer.is_garbage() {
                println!("[dap] too many garbage bytes, drop connection");
                return;
            }
        }
    }
}

/// 处理一帧 DAP 请求，返回响应长度。
/// TODO 下一轮：换成 CMSIS-DAP 命令表
fn handle(req: &[u8], resp: &mut [u8]) -> usize {
    let n = req.len().min(resp.len());
    resp[..n].copy_from_slice(&req[..n]);
    n
}