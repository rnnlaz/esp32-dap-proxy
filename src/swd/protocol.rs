use esp_println::println;

use super::SwdIo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwdError {
    AckWait,
    AckFault,
    AckUnknown(u8),
    ParityError,
}

pub struct SwdProtocol<'a> {
    io: SwdIo<'a>,
}

impl <'a> SwdProtocol<'a> {

    pub fn new(swd_io: SwdIo<'a>) -> Self {
        Self { io: swd_io }
    }

    pub fn free(self) -> SwdIo<'a> {
        self.io
    }

    fn make_request(&self, apndp: bool, rnw: bool, addr: u8) -> u8 {
        let start = 1u8;
        let apndp_bit = (apndp as u8) << 1;
        let rnw_bit = (rnw as u8) << 2;
        let addr_bit = ((addr >> 2) & 0x03) << 3;
        let payload = apndp_bit | rnw_bit | addr_bit;
        let parity_bit = ((payload.count_ones() % 2) as u8) << 5;
        let stop = 0u8 << 6;
        let park = 1u8 << 7;

        start | payload | parity_bit | stop | park
    }

    fn idle(&mut self) {
        self.io.write_bits(0x00, 8);
    }

    #[inline(always)]
    fn turnaround(&mut self) {
        if self.io.is_swdio_output() {
            self.io.set_swdio_input();
            self.io.clock_cycle(); // 测试f103时，需注释本此clock
        } else {
            self.io.clock_cycle();
            self.io.set_swdio_output();
        }
    }

    pub fn sync(&mut self) {
        self.io.line_reset();
        self.io.write_bits( 0x79E7u16.reverse_bits().into(), 16);
        self.io.line_reset();

        self.idle();
    }

    pub fn read_dp(&mut self, addr: u8) -> Result<u32, SwdError> {
        let request = self.make_request(false, true, addr);

        self.io.write_bits(request as u32, 8);

        self.turnaround();

        let ack = self.io.read_bits(3) as u8;

        if ack != 0b001 {
            self.turnaround();

            return match ack {
                0b100 => Err(SwdError::AckFault),
                0b010 => Err(SwdError::AckWait),
                _ => Err(SwdError::AckUnknown(ack)),
            }
        }

        let data = self.io.read_bits(32);
        let parity_bit = self.io.read_bit();

        self.turnaround();

        self.idle();

        let expected_parity = (data.count_ones() % 2) != 0;
        if parity_bit != expected_parity {
            return Err(SwdError::ParityError);
        }

        Ok(data)
    }

    pub fn write_dp(&mut self, addr: u8, data: u32) -> Result<(), SwdError> {
        let request = self.make_request(false, false, addr);

        self.io.write_bits(request as u32, 8);

        self.turnaround();

        let ack = self.io.read_bits(3) as u8;

        if ack != 0b001 {
            self.turnaround();

            return match ack {
                0b100 => Err(SwdError::AckFault),
                0b010 => Err(SwdError::AckWait),
                _ => Err(SwdError::AckUnknown(ack)),
            }
        }

        self.turnaround();

        self.idle();

        self.io.write_bits(data, 32);
        let parity = (data.count_ones() % 2) != 0;
        self.io.write_bit(parity);

        Ok(())        
    }
}