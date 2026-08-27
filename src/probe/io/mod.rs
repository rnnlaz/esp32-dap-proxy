pub mod bitbang;

pub trait Io {
    fn set_clk_high(&mut self);
    fn set_clk_low(&mut self);

    fn set_data_high(&mut self);
    fn set_data_low(&mut self);

    fn is_data_high(&self) -> bool;

    fn set_data_output(&mut self);
    fn set_data_input(&mut self);

    fn clock_cycle(&mut self);

    fn init(&mut self) {
        self.set_clk_low();
        self.set_data_high();
        self.set_data_output();
    }

    fn line_reset(&mut self) {
        self.set_data_high();
        self.set_data_output();
        for _ in 0..60 {
            self.clock_cycle();
        }
    }

    #[inline(always)]
    fn write_bit(&mut self, bit: bool) {
        if bit {
            self.set_data_high();
        } else {
            self.set_data_low();
        }
        self.clock_cycle();
    }

    #[inline(always)]
    fn read_bit(&mut self) -> bool {
        self.set_clk_low();
        let bit = self.is_data_high();
        self.set_clk_high();
        bit
    }

    fn write_u32(&mut self, value: u32, num_bits: u8) {
        for i in 0..num_bits {
            let bit = (value >> i) & 1 != 0;
            self.write_bit(bit);
        }
    }

    fn read_u32(&mut self, num_bits: u8) -> u32 {
        let mut value = 0;
        for i in 0..num_bits {
            if self.read_bit() {
                value |= 1 << i;
            }
        }
        value
    }
}