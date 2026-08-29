pub const FRAME_MAGIC: u8 = 0xDA;
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
                self.state = if len == 0 {
                    RxState::Magic
                } else {
                    RxState::Payload { left: len }
                };
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
                    self.rejects = 0;
                    Some(&self.buf[..n])
                } else {
                    self.state = RxState::Payload { left: left - 1 };
                    None
                }
            }
        }
    }
}
