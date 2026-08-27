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

use crate::probe::target::cortex_m::*;

#[panic_handler]
fn panic(info : &core::panic::PanicInfo) -> ! {
    println!("Panic!: {:?}", info);
    loop {}
}

mod probe;

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

    let swd_io = probe::io::bitbang::BitBangIo::new(swclk, swdio, None);
    let swd = probe::transport::swd::Swd::new(swd_io);
    let mut swd_probe = probe::Probe::new(swd);

    println!("Starting SWD probe...");
    let id = swd_probe.connect().expect("Failed to connect to target");
    println!("Connected to target with ID: 0x{:08X}", id);

    let sp = swd_probe.read32(0x0000_0000).expect("read SP failed");
    println!("SP: 0x{:08X}", sp);

    let pc = swd_probe.read32(0x0000_0004).expect("read PC failed");
    println!("PC: 0x{:08X}", pc);

    let mut buf = [0u32; 64];
    swd_probe.read_bulk(0x0800_0000, &mut buf).expect("bulk read failed");
    println!("First word of flash: 0x{:08X}", buf[0]);

    let mut target = probe::target::cortex_m::CortexM::new(&mut swd_probe);

    target.halt().expect("Failed to halt target");
    println!("Target halted.");

    let pc = target.read_register(REG_PC).expect("Failed to read PC");
    println!("PC after halt: 0x{:08X}", pc);

    target.step().expect("Failed to step target");
    println!("Target stepped.");

    let pc1 = target.read_register(REG_PC).expect("Failed to read PC after step");
    println!("PC after step: 0x{:08X}", pc1);

    target.write_register(REG_PC, pc).expect("Failed to write PC");
    let pc3 = target.read_register(REG_PC).expect("Failed to read PC after write");
    println!("PC after write: 0x{:08X}", pc3);

    let regs = target.read_core_registers().expect("Failed to read core registers");
    for i in 0..regs.len() {
        println!("R{}: 0x{:08X}", i, regs[i]);
    }
    println!("XPSR: 0x{:08X}", regs[16]);

    let frame = target.read_current_stack_frame().expect("Failed to read current stack frame");
    println!("Current stack frame:");
    for i in 0..frame.len() {
        println!("Stack[{}]: 0x{:08X}", i, frame[i]);
    }

    target.resume().expect("Failed to resume target");
    println!("Target resumed.");

    loop {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {}
    }
    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}
