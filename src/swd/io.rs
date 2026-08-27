
use esp_hal::gpio::{DriveMode, Flex, InputConfig, Level, Output, OutputConfig, Pull};
use esp_println::println;

pub struct SwdIo<'a> {
    swclk: Output<'a>,
    swdio: Flex<'a>,
    is_output: bool,
}

impl<'a> SwdIo<'a> {

    const DELAY_CYCLES: u32 = 20;

    pub fn new(mut swclk: Output<'a>, mut swdio: Flex<'a>) -> Self {
        let out_config = OutputConfig::default().with_drive_mode(DriveMode::OpenDrain).with_pull(Pull::Up);
        let in_config = InputConfig::default().with_pull(Pull::Up);

        swclk.set_low();

        swdio.apply_output_config(&out_config);
        swdio.apply_input_config(&in_config);

        swdio.set_input_enable(true);
        swdio.set_output_enable(true);
        swdio.set_high();

        Self {
            swclk,
            swdio,
            is_output: true,
        }
    }

    #[inline(always)]
    pub fn clk_delay(&mut self) {
        for _ in 0..Self::DELAY_CYCLES { core::hint::black_box(()); }
    }

    #[inline(always)]
    pub fn clock_high(&mut self) {
        self.swclk.set_high();
        self.clk_delay();
    }

    #[inline(always)]
    pub fn clock_low(&mut self) {
        self.swclk.set_low();
        self.clk_delay();
    }

    #[inline(always)]
    pub fn clock_cycle(&mut self) {
        self.clock_low();
        self.clock_high();
    }

    #[inline(always)]
    pub fn is_swdio_output(&self) -> bool {
        self.is_output
    }

    #[inline(always)]
    pub fn set_swdio_output(&mut self) {
        self.swdio.set_high();
        self.swdio.set_output_enable(true);
        self.is_output = true;
    }

    #[inline(always)]
    pub fn set_swdio_input(&mut self) {
        self.swdio.set_output_enable(false);
        self.is_output = false;
    }

    pub fn line_reset(&mut self) {
        self.swdio.set_high();
        for _ in 0..60 {
            self.clock_cycle();
        }
    }

    #[inline(always)]
    pub fn write_bit(&mut self, bit: bool) {
        self.swdio.set_level(if bit { Level::High } else { Level::Low });
        self.clock_cycle();
    }

    #[inline(always)]
    pub fn read_bit(&mut self) -> bool {
        self.clock_low();
        let bit = self.swdio.is_high();
        self.clock_high();
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