use super::protocol::{SwdError, SwdProtocol};

pub const DP_REG_DPIDR: u8 = 0x00;
pub const DP_REG_CTRL_STAT: u8 = 0x04;
pub const DP_REG_SELECT: u8 = 0x08;
pub const DP_REG_RDBUFF: u8 = 0x0C;

pub const CDBGPWRUPREQ: u32 = 1 << 28;
pub const CDBGPWRUPACK: u32 = 1 << 29;
pub const CSYSPWRUPREQ: u32 = 1 << 30;
pub const CSYSPWRUPACK: u32 = 1 << 31;

impl<'a> SwdProtocol<'a> {
    pub fn read_dpidr(&mut self) -> Result<u32, SwdError> {
        self.read_dp(DP_REG_DPIDR)
    }

    pub fn dp_power_up(&mut self) -> Result<(), SwdError> {
        let pwr_req = CDBGPWRUPREQ | CSYSPWRUPREQ;
        self.write_dp(DP_REG_CTRL_STAT, pwr_req)?;

        for _ in 0..1000 { core::hint::black_box(()); }
        for _ in 0..100 {
            let stat = self.read_dp(DP_REG_CTRL_STAT)?;
            if (stat & (CDBGPWRUPACK | CSYSPWRUPACK)) == (CDBGPWRUPACK | CSYSPWRUPACK) {
                return Ok(());
            }
        }
        
        Err(SwdError::AckWait)
    }

    pub fn select_ap_bank(&mut self, ap_num: u8, bank: u8) -> Result<(), SwdError> {
        let select_val = ((ap_num as u32) << 24) | (((bank & 0x0F) as u32) << 4);
        self.write_dp(DP_REG_SELECT, select_val)
    }
}
