#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::{
    clock::CpuClock, gpio::*, main, time::{Duration, Instant},
};

use esp_println::println;

mod swd;
use swd::SwdIo;
use swd::SwdProtocol;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}


// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32c3 -o nightly-x86_64-pc-windows-gnu -o vscode -o esp32c3-wroom-02 -o unstable-hal


    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let swclk = Output::new(peripherals.GPIO0, Level::Low, OutputConfig::default());
    let swdio = Flex::new(peripherals.GPIO1);

    let swd_io = SwdIo::new(swclk, swdio);
    let mut swd = SwdProtocol::new(swd_io);
    swd.sync();
    
    loop {
        match swd.read_dpidr() {
            Ok(dpidr) => {
                println!("DPIDR: {:#010X}", dpidr);
                match swd.dp_power_up() {
                    Ok(()) => println!("DP power up successful"),
                    Err(e) => println!("Error powering up DP: {:?}", e),
                }
                swd.read_ctrl_stat().map(|ctrl_stat| {
                    println!("CTRL/STAT: {:#010X}", ctrl_stat);
                }).unwrap_or_else(|e| {
                    println!("Error reading CTRL/STAT: {:?}", e);
                });
            }
            Err(e) => {
                println!("Error reading DPIDR: {:?}", e);
            }
        }

        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {}
    }
    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}
