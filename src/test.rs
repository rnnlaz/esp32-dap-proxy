
pub fn test_probe() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);

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

    let mut target = probe::target::cortex_m::CortexM::new(&mut swd_probe).expect("Failed to create Cortex-M target");

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

    target.halt().expect("1");
    let a = target.read_register(REG_PC).expect("2");
    println!("PC at halt: 0x{:08X}", a);

    target.step().expect("9");
    let b = target.read_register(REG_PC).expect("3");
    println!("PC after step: 0x{:08X}", b);

    target.set_breakpoint(a).expect("4");
    println!("BP at 0x{:08X}", a);

    target.resume().expect("5");
    target.wait_for_halt().expect("6");
    let pc = target.read_register(REG_PC).expect("7");
    println!("Hit BP, PC = 0x{:08X}", pc);
}