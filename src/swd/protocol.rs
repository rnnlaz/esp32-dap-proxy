use super::SwdIo;

impl <'a> SwdIo<'a> {
    pub fn line_reset(&mut self) {
        self.set_swdio_output();
        for _ in 0..60 {
            self.write_bit(true);
        }
    }

    pub fn write_abort(&mut self, abort_value: u32) {
        self.set_swdio_output();
        self.write_bits(0x81, 8);

        self.set_swdio_input();
        self.clock_cycle();

        let _ack = self.read_bits(3);

        self.clock_cycle();
        self.set_swdio_output();

        self.write_bits(abort_value, 32);
        let parity = (abort_value.count_ones() % 2) != 0;
        self.write_bit(parity);
    }

    pub fn switch_jtag_to_swd(&mut self) {
        self.line_reset();
        self.write_bits(0xE79E, 16);
        self.line_reset();

        self.set_swdio_output();
        self.write_bits(0x00, 8);
    }

    pub fn read_dpidr(&mut self) -> Result<u32, &'static str> {
        self.switch_jtag_to_swd();
        self.set_swdio_output();
        self.write_bits(0xA5, 8);

        self.set_swdio_input();
        // self.clock_cycle(); // 测试后发现不加此行能正确读取

        let ack = self.read_bits(3);
        if ack != 0b001 {
            self.clock_cycle();
            self.set_swdio_output();

            return match ack {
                0b100 => Err("SWD protocol error: FAULT response"),
                0b010 => Err("SWD protocol error: WAIT response"),
                _ => Err("SWD protocol error: Invalid ACK response"),
            }
        }

        let dpidr = self.read_bits(32);
        let parity_bit = self.read_bit();

        self.clock_cycle();
        self.set_swdio_output();

        let expected_parity = (dpidr.count_ones() % 2) != 0;
        if parity_bit != expected_parity {
            return Err("SWD protocol error: Parity mismatch");
        }

        Ok(dpidr)
    }
}