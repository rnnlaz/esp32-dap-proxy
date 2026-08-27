pub mod io;
pub mod target;
pub mod transport;

use esp_println::println;
use target::dp::*;
use target::ap::*;
use target::cortex_m::*;
use transport::Transport;

pub struct Probe<T: Transport> {
    transport: T,
}

impl<T: Transport> Probe<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn connect(&mut self) -> Result<u32, transport::Error> {
        self.transport.init()?;
        let id = self.transport.read_dp(DP_DPIDR)?;
        self.transport.write_dp(DP_CTRL_STAT, CDBGPWRUPREQ | CSYSPWRUPREQ)?;
        for _ in 0..1000 {
            let stat = self.transport.read_dp(DP_CTRL_STAT)?;
            if (stat & (CDBGPWRUPACK | CSYSPWRUPACK)) == (CDBGPWRUPACK | CSYSPWRUPACK) {
                return Ok(id);
            }
        }
        Err(transport::Error::Io)
    }

    pub fn reset(&mut self) -> Result<(), transport::Error> {
        self.transport.reset_state();
        self.transport.write_dp(DP_SELECT, 0x0000_0004)?;
        self.transport.write_dp(DP_ABORT, 0x1F)?;
        Ok(())
    }
}

impl<T: Transport> Probe<T> {
    pub fn read32(&mut self, addr: u32) -> Result<u32, transport::Error> {
        self.transport.write_ap(0, AP_CSW, CSW_32_OFF)?;
        self.transport.write_ap(0, AP_TAR, addr)?;
        self.transport.read_ap(0, AP_DRW)
    }

    pub fn write32(&mut self, addr: u32, value: u32) -> Result<(), transport::Error> {
        self.transport.write_ap(0, AP_CSW, CSW_32_OFF)?;
        self.transport.write_ap(0, AP_TAR, addr)?;
        self.transport.write_ap(0, AP_DRW, value)?;
        Ok(())
    }

    pub fn read_bulk(&mut self, addr: u32, buffer: &mut [u32]) -> Result<(), transport::Error> {
        self.transport.write_ap(0, AP_CSW, CSW_32_SINGLE)?;
        self.transport.write_ap(0, AP_TAR, addr)?;

        self.transport.read_ap(0, AP_DRW)?; // Dummy read

        for i in 0..buffer.len() {
            buffer[i] = self.transport.read_ap(0, AP_DRW)?;
        }

        Ok(())
    }

    pub fn write_bulk(&mut self, addr: u32, buffer: &[u32]) -> Result<(), transport::Error> {
        self.transport.write_ap(0, AP_CSW, CSW_32_SINGLE)?;
        self.transport.write_ap(0, AP_TAR, addr)?;

        for &value in buffer {
            self.transport.write_ap(0, AP_DRW, value)?;
        }

        Ok(())
    }
}

impl<T: Transport> Probe<T> {
    pub fn clear_sticky(&mut self) -> Result<(), transport::Error> {
        println!("clear_sticky: writing ABORT");
        let ab = self.transport.write_dp(DP_ABORT, 0x1F);
        println!("clear_sticky: ABORT write => {:?}", ab);
        ab
    }

    pub fn halt(&mut self) -> Result<(), transport::Error> {
        self.write32(DHCSR, DBGKEY | C_DEBUGEN | C_HALT)?;

        for _ in 0..1000 {
            match self.read32(DHCSR) {
                Ok(dhcsr) if dhcsr & S_HALT != 0 => return Ok(()),
                Ok(_) => {}
                Err(transport::Error::Fault) => self.clear_sticky()?,
                Err(e) => return Err(e),
            }
        }
        Err(transport::Error::Io)
    }

    pub fn resume(&mut self) -> Result<(), transport::Error> {
        self.write32(DHCSR, DBGKEY | C_DEBUGEN)?;

        for _ in 0..1000 {
            let dhcsr = self.read32(DHCSR)?;
            if (dhcsr & S_HALT) == 0 {
                return Ok(());
            }
        }
        Err(transport::Error::Io)
    }

    pub fn read_register(&mut self, n: u16) -> Result<u32, transport::Error> {
        self.write32(DCRSR, n as u32)?;
        
        for _ in 0..1000 {
            match self.read32(DHCSR) {
                Ok(dhcsr) if dhcsr & S_REGRDY != 0 => {
                    return self.read32(DCRDR);
                }
                Ok(_) => {}
                Err(transport::Error::Fault) => self.clear_sticky()?,
                Err(e) => return Err(e),
            }
        }
        self.read32(DCRDR)
    }
}