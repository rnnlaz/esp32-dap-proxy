use esp_hal::gpio::{DriveMode, Flex, InputConfig, Output, OutputConfig, Pull};

use super::Io;

pub struct BitBangIo<'a> {
    swclk: Output<'a>,
    swdio: Flex<'a>,
    delay: u32,
}

impl<'a> BitBangIo<'a> {
    const DEFAULT_DELAY: u32 = 20;

    pub fn new(mut swclk: Output<'a>, mut swdio: Flex<'a>, delay: Option<u32>) -> Self {
        let out_cfg = OutputConfig::default()
            .with_drive_mode(DriveMode::OpenDrain)
            .with_pull(Pull::Up);
        let in_cfg = InputConfig::default().with_pull(Pull::Up);

        swclk.set_low();

        swdio.apply_output_config(&out_cfg);
        swdio.apply_input_config(&in_cfg);
        swdio.set_high();
        swdio.set_input_enable(true);
        swdio.set_output_enable(true);

        Self {
            swclk,
            swdio,
            delay: delay.unwrap_or(Self::DEFAULT_DELAY),
        }
    }

    fn nop_delay(&self) {
        for _ in 0..self.delay {
            core::hint::black_box(());
        }
    }
}

impl Io for BitBangIo<'_> {
    fn set_clk_high(&mut self) {
        self.swclk.set_high();
        self.nop_delay();
    }

    fn set_clk_low(&mut self) {
        self.swclk.set_low();
        self.nop_delay();
    }

    fn set_data_high(&mut self) {
        self.swdio.set_high();
    }

    fn set_data_low(&mut self) {
        self.swdio.set_low();
    }

    fn is_data_high(&self) -> bool {
        self.swdio.is_high()
    }

    fn set_data_output(&mut self) {
        self.swdio.set_output_enable(true);
    }

    fn set_data_input(&mut self) {
        self.swdio.set_output_enable(false);
    }

    fn clock_cycle(&mut self) {
        self.set_clk_low();
        self.set_clk_high();
    }
}
