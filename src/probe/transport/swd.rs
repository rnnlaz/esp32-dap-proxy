use crate::probe::{io::Io, target::dp::{DP_RDBUFF, DP_SELECT}, transport::{Error, Transport}};

pub struct Swd<I: Io> {
    io: I,
    select_cache: u32,
    driving: bool,
}

impl<I: Io> Swd<I> {
    pub fn new(io: I) -> Self {
        Self {
            io,
            select_cache: !0,
            driving: true,
        }
    }
}

impl<I: Io> Swd<I> {
    fn make_request(apndp: bool, rnw: bool, addr: u8) -> u8 {
        let start = 1u8;
        let apndp_bit = (apndp as u8) << 1;
        let rnw_bit = (rnw as u8) << 2;
        let addr_bits = ((addr >> 2) & 0x03) << 3;
        let payload = apndp_bit | rnw_bit | addr_bits;
        let parity_bit = ((payload.count_ones() % 2) as u8) << 5;
        let stop = 0u8 << 6;
        let park = 1u8 << 7;

        start | payload | parity_bit | stop | park
    }

    fn idle(&mut self) {
        self.io.write_u32(0x00, 8);
    }

    fn turnaround(&mut self) {
        if self.driving {
            self.io.set_data_input();
            self.io.clock_cycle();
            self.driving = false;
        } else {
            self.io.clock_cycle();
            self.io.set_data_output();
            self.driving = true;
        }
    }

    fn transfer(&mut self, apndp: bool, rnw: bool, addr: u8, value: Option<u32>) -> Result<u32, Error> {
        let request = Self::make_request(apndp, rnw, addr);
        self.io.write_u32(request as u32, 8);

        self.turnaround();

        let ack = self.io.read_u32(3) as u8;
        match ack {
            0b001 => {}
            0b010 => {self.turnaround(); return Err(Error::Wait); }
            0b100 => {self.turnaround(); return Err(Error::Fault); }
            _ => {self.turnaround(); return Err(Error::Unknown(ack)); }
        }

        let data = if rnw {
            let data = self.io.read_u32(32);
            let partity =  self.io.read_bit();

            self.turnaround();

            if partity != (data.count_ones() % 2 != 0) {
                return Err(Error::Parity);
            }

            data
        } else {
            self.turnaround();

            let data = value.unwrap_or(0);
            self.io.write_u32(data, 32);
            let partity = data.count_ones() % 2 != 0;
            self.io.write_bit(partity);

            data
        };

        self.idle();

        Ok(data)
    }

    fn select_ap(&mut self, ap_select: u8, addr: u8) -> Result<(), Error> {
        let bank = (addr >> 4) & 0x0F;
        let select = ((ap_select as u32) << 24) | ((bank as u32) << 4);
        if select != self.select_cache {
            self.write_dp(DP_SELECT, select)?;
            self.select_cache = select;
        }
        Ok(())
    }

    fn read_ap_raw(&mut self, select: u8, addr: u8) -> Result<u32, Error> {
        self.select_ap(select, addr)?;
        self.transfer(true, true, addr, None)
    }

    fn read_rdbuff(&mut self) -> Result<u32, Error> {
        self.read_dp(DP_RDBUFF)
    }
}

impl<I: Io> Transport for Swd<I> {
    fn init(&mut self) -> Result<(), Error> {
        self.io.line_reset();
        self.io.write_u32(0xE79E, 16);
        self.io.line_reset();
        self.idle();

        self.select_cache = !0;
        self.driving = true;
        Ok(())
    }

    fn read_dp(&mut self, addr: u8) -> Result<u32, Error> {
        self.transfer(false, true, addr, None)
    }

    fn write_dp(&mut self, addr: u8, val: u32) -> Result<(), Error> {
        self.transfer(false, false, addr, Some(val))?;
        Ok(())
    }

    fn read_ap(&mut self, select: u8, addr: u8) -> Result<u32, Error> {
        self.read_ap_raw(select, addr)?;
        self.read_rdbuff()
    }

    fn write_ap(&mut self, select: u8, addr: u8, data: u32) -> Result<(), Error> {
        self.select_ap(select, addr)?;
        self.transfer(true, false, addr, Some(data))?;
        Ok(())
    }

    fn reset_state(&mut self) {
        self.select_cache = !0;
    }
}