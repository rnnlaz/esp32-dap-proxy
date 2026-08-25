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

    fn sync(&mut self) {
        self.io.line_reset();
        self.io.write_bits(0xE79E, 16);
        self.io.line_reset();

        self.io.set_swdio_output();
        self.io.write_bits(0x00, 8);
    }

    fn read_dp(&mut self, addr: u8) -> Result<u32, SwdError> {
        let request = self.make_request(false, true, addr);

        self.io.set_swdio_output();
        self.io.write_bits(request as u32, 8);

        self.io.set_swdio_input();
        // self.io.clock_cycle(); // 测试后发现不显示等待turnaround，才能读取Ack

        let ack = self.io.read_bits(3) as u8;

        if ack != 0b001 {
            self.io.clock_cycle();
            self.io.set_swdio_output();

            return match ack {
                0b100 => Err(SwdError::AckFault),
                0b010 => Err(SwdError::AckWait),
                _ => Err(SwdError::AckUnknown(ack)),
            }
        }

        let data = self.io.read_bits(32);
        let parity_bit = self.io.read_bit();

        self.io.clock_cycle();
        self.io.set_swdio_output();

        let expected_parity = (data.count_ones() % 2) != 0;
        if parity_bit != expected_parity {
            return Err(SwdError::ParityError);
        }

        Ok(data)
    }

    fn write_dp(&mut self, addr: u8, data: u32) -> Result<(), SwdError> {
        let request = self.make_request(false, false, addr);

        self.io.set_swdio_output();
        self.io.write_bits(request as u32, 8);

        self.io.set_swdio_input();
        // self.io.clock_cycle(); // 测试后发现不显示等待turnaround，才能读取Ack

        let ack = self.io.read_bits(3) as u8;

        if ack != 0b001 {
            self.io.clock_cycle();
            self.io.set_swdio_output();

            return match ack {
                0b100 => Err(SwdError::AckFault),
                0b010 => Err(SwdError::AckWait),
                _ => Err(SwdError::AckUnknown(ack)),
            }
        }

        self.io.clock_cycle();
        self.io.set_swdio_output();

        self.io.write_bits(data, 32);
        let parity = (data.count_ones() % 2) != 0;
        self.io.write_bit(parity);

        Ok(())        
    }

    pub fn read_dpidr(&mut self) -> Result<u32, SwdError> {
        self.sync();
        self.read_dp(0x0)
    }

    pub fn write_abort(&mut self, abort_val: u32) -> Result<(), SwdError> {
        self.write_dp(0x00, abort_val)
    }
}