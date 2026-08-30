pub mod swd;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Wait,
    Fault,
    Unknown(u8),
    Parity,
    Io,
}

pub trait Transport {
    fn init(&mut self) -> Result<(), Error>;
    fn reset_state(&mut self);

    fn swj_sequence(&mut self, count: u8, data: &[u8]) -> Result<(), Error>;
    fn swj_pins(&mut self, pin_out: u8, pin_sel: u8, wait_ms: u32) -> Result<u8, Error>;

    fn read_dp(&mut self, addr: u8) -> Result<u32, Error>;
    fn write_dp(&mut self, addr: u8, val: u32) -> Result<(), Error>;

    fn read_ap(&mut self, select: u8, addr: u8) -> Result<u32, Error>;
    fn write_ap(&mut self, select: u8, addr: u8, data: u32) -> Result<(), Error>;
}
