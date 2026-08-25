
use esp_hal::gpio::{DriveMode::OpenDrain, Flex, InputConfig, Level, Output, OutputConfig, Pull};

pub struct SwdIo<'a> {
    swclk: Output<'a>,
    swdio: Flex<'a>
}

impl<'a> SwdIo<'a> {
    pub fn new(mut swclk: Output<'a>, mut swdio: Flex<'a>) -> Self {
        let out_config = OutputConfig::default().with_drive_mode(OpenDrain).with_pull(Pull::Up);
        let in_config = InputConfig::default().with_pull(Pull::Up);

        swdio.apply_output_config(&out_config);
        swdio.apply_input_config(&in_config);
        swclk.set_low();

        Self {
            swclk,
            swdio,
        }
    }

    #[inline(always)]
    pub fn clock_high(&mut self) {
        self.swclk.set_high();
    }

    #[inline(always)]
    pub fn clock_low(&mut self) {
        self.swclk.set_low();
        for _ in 0..200 { core::hint::black_box(()); }
    }

    #[inline(always)]
    pub fn clock_cycle(&mut self) {
        self.clock_high();
        self.clock_low();
    }

    #[inline(always)]
    pub fn set_swdio_output(&mut self) {
        self.swdio.set_output_enable(true);
        self.swdio.set_input_enable(false);
    }

    #[inline(always)]
    pub fn set_swdio_input(&mut self) {
        self.swdio.set_input_enable(true);
        self.swdio.set_output_enable(false);
    }

    #[inline(always)]
    pub fn write_bit(&mut self, bit: bool) {
        self.swdio.set_level(if bit { Level::High } else { Level::Low });
        self.clock_cycle();
    }

    #[inline(always)]
    pub fn read_bit(&mut self) -> bool {
        self.clock_high();
        let bit = self.swdio.is_high();
        self.clock_low();
        bit
    }

    pub fn write_bits(&mut self, mut value: u32, num_bits: u8) {
        for _ in 0..num_bits {
            self.write_bit((value & 1) != 0);
            value >>= 1;
        }
    }

    pub fn read_bits(&mut self, num_bits: u8) -> u32 {
        let mut value = 0;
        for i in 0..num_bits {
            if self.read_bit() {
                value |= 1 << i;
            }
        }
        value
    }
}