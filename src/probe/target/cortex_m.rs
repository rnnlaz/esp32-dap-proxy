pub const DHCSR: u32 = 0xE000_EDF0;
pub const DCRSR: u32 = 0xE000_EDF4;
pub const DCRDR: u32 = 0xE000_EDF8;
pub const DEMCR: u32 = 0xE000_EDFC;

pub const DBGKEY:       u32 = 0xA05F << 16;
pub const C_DEBUGEN:    u32 = 1 << 0;
pub const C_HALT:       u32 = 1 << 1;
pub const C_STEP:       u32 = 1 << 2;
pub const C_MASKINTS:   u32 = 1 << 3;
pub const C_SNAPSTALL:  u32 = 1 << 16;

pub const S_REGRDY:     u32 = 1 << 16;
pub const S_HALT:       u32 = 1 << 17;
pub const S_SLEEP:      u32 = 1 << 18;
pub const S_LOCKUP:     u32 = 1 << 19;
pub const S_RETIRE_ST:  u32 = 1 << 24;
pub const S_RESET_ST:   u32 = 1 << 25;
pub const S_CONTROL:    u32 = 1 << 28;
pub const S_PRIMASK:    u32 = 1 << 29;
pub const S_FAULTMASK:  u32 = 1 << 30;

// DCRSR
pub const DCRSR_REGWNR: u32 = 1 << 16;

// DEMCR
pub const VC_CORERESET: u32 = 1 << 0;
pub const VC_MMERR:     u32 = 1 << 4;
pub const VC_NOCPERR:   u32 = 1 << 5;
pub const VC_CHKERR:    u32 = 1 << 6;
pub const VC_STATERR:   u32 = 1 << 8;
pub const VC_BUSERR:    u32 = 1 << 9;
pub const VC_INTERR:    u32 = 1 << 10;
pub const VC_HARDERR:   u32 = 1 << 11;

// DCRSR.REGSEL
pub const REG_R0:       u16 = 0x00;
pub const REG_R1:       u16 = 0x01;
pub const REG_R2:       u16 = 0x02;
pub const REG_R3:       u16 = 0x03;
pub const REG_R4:       u16 = 0x04;
pub const REG_R5:       u16 = 0x05;
pub const REG_R6:       u16 = 0x06;
pub const REG_R7:       u16 = 0x07;
pub const REG_R8:       u16 = 0x08;
pub const REG_R9:       u16 = 0x09;
pub const REG_R10:      u16 = 0x0A;
pub const REG_R11:      u16 = 0x0B;
pub const REG_R12:      u16 = 0x0C;
pub const REG_SP:       u16 = 0x0D;   // 当前 SP（MSP 或 PSP，看 S_CONTROL）
pub const REG_LR:       u16 = 0x0E;
pub const REG_PC:       u16 = 0x0F;
pub const REG_XPSR:     u16 = 0x10;
pub const REG_MSP:      u16 = 0x11;
pub const REG_PSP:      u16 = 0x12;

pub const REG_PRIMASK:    u16 = 0x14;
pub const REG_BASEPRI:    u16 = 0x15;
pub const REG_BASEPRI_MAX:u16 = 0x16;
pub const REG_FAULTMASK:  u16 = 0x17;
pub const REG_CONTROL:    u16 = 0x18;

// FPU
pub const REG_S0:  u16 = 0x40;
pub const REG_S16: u16 = 0x60;

// FPB
pub const FP_CTRL:  u32 = 0xE000_2000;
pub const FP_COMP0: u32 = 0xE000_2008;

// FP_CTRL
pub const FPB_ENABLE: u32 = 1 << 0;
pub const FPB_KEY:    u32 = 1 << 1;
pub const FPB_NUM_CODE_MASK: u32 = 0x1F << 4;

// FP_COMPx
pub const FPB_BP_BOTH: u32 = 0b11 << 30;
pub const FPB_BP_LOWER: u32 = 0b01 << 30;
pub const FPB_BP_UPPER: u32 = 0b10 << 30;
pub const FPB_COMP_ENABLE: u32 = 1 << 0;

use esp_println::println;

use super::super::Probe;
use super::super::transport;

pub struct CortexM<'a, T: transport::Transport> {
    probe: &'a mut Probe<T>,
    breakpoints_slots: [Option<u32>; 6],
    breakpoints_num: usize,
}

impl<'a, T: transport::Transport> CortexM<'a, T> {
    pub fn new(probe: &'a mut Probe<T>) -> Result<Self, transport::Error> {
        let ctrl = probe.read32(FP_CTRL)?;
        let breakpoints_num = ((ctrl & FPB_NUM_CODE_MASK) >> 4) as usize;
        Ok(CortexM{
            probe,
            breakpoints_slots: [None; 6],
            breakpoints_num,
        })
    }

    pub fn read_register(&mut self, n: u16) -> Result<u32, transport::Error> {
        self.probe.write32(DCRSR, n as u32)?;
        
        for _ in 0..1000 {
            match self.probe.read32(DHCSR) {
                Ok(dhcsr) if dhcsr & S_REGRDY != 0 => {
                    return self.probe.read32(DCRDR);
                }
                Ok(_) => {}
                Err(transport::Error::Fault) => self.probe.clear_sticky()?,
                Err(e) => return Err(e),
            }
        }
        self.probe.read32(DCRDR)
    }

    pub fn write_register(&mut self, n: u16, value: u32) -> Result<(), transport::Error> {
        self.probe.write32(DCRDR, value)?;
        self.probe.write32(DCRSR, n as u32 | DCRSR_REGWNR)?;

        for _ in 0..1000 {
            match self.probe.read32(DHCSR) {
                Ok(dhcsr) if dhcsr & S_REGRDY != 0 => return Ok(()),
                Ok(_) => {}
                Err(transport::Error::Fault) => self.probe.clear_sticky()?,
                Err(e) => return Err(e),
            }
        }
        Err(transport::Error::Io)
    }

    pub fn read_core_registers(&mut self) -> Result<[u32; 17], transport::Error> {
        let mut regs = [0u32; 17];
        for i in 0..17 {
            regs[i] = self.read_register(i as u16)?;
        }
        Ok(regs)
    }

    pub fn read_stack_frame(&mut self, sp: u32) -> Result<[u32; 8], transport::Error> {
        let mut frame = [0u32; 8];
        self.probe.read_bulk(sp, &mut frame)?;
        Ok(frame)
    }

    pub fn read_current_stack_frame(&mut self) -> Result<[u32; 8], transport::Error> {
        let sp = self.read_register(REG_MSP)?;
        self.read_stack_frame(sp)
    }
}

impl<'a, T: transport::Transport> CortexM<'a, T> {
    pub fn halt(&mut self) -> Result<(), transport::Error> {
        self.probe.write32(DHCSR, DBGKEY | C_DEBUGEN | C_HALT)?;

        for _ in 0..1000 {
            match self.probe.read32(DHCSR) {
                Ok(dhcsr) if dhcsr & S_HALT != 0 => return Ok(()),
                Ok(_) => {}
                Err(transport::Error::Fault) => self.probe.clear_sticky()?,
                Err(e) => return Err(e),
            }
        }
        Err(transport::Error::Io)
    }

    pub fn step(&mut self) -> Result<(), transport::Error> {
        self.probe.write32(DHCSR, DBGKEY | C_DEBUGEN | C_STEP)?;

        for _ in 0..1000 {
            match self.probe.read32(DHCSR) {
                Ok(dhcsr) if dhcsr & S_HALT != 0 => return Ok(()),
                Ok(_) => {}
                Err(transport::Error::Fault) => self.probe.clear_sticky()?,
                Err(e) => return Err(e),
            }
        }
        Err(transport::Error::Io)
    }

    pub fn wait_for_halt(&mut self) -> Result<(), transport::Error> {
        for _ in 0..10000 { // TODO: Make this for interrupts, not a busy wait
            match self.probe.read32(DHCSR) {
                Ok(dhcsr) if dhcsr & S_HALT != 0 => return Ok(()),
                Ok(_) => {}
                Err(transport::Error::Fault) => self.probe.clear_sticky()?,
                Err(e) => return Err(e),
            }
        }
        Err(transport::Error::Io)
    }

    pub fn resume(&mut self) -> Result<(), transport::Error> {
        self.probe.write32(DHCSR, DBGKEY | C_DEBUGEN)?;

        // for _ in 0..1000 {
        //     match self.probe.read32(DHCSR) {
        //         Ok(dhcsr) if dhcsr & S_HALT == 0 => return Ok(()),
        //         Ok(_) => {}
        //         Err(transport::Error::Fault) => self.probe.clear_sticky()?,
        //         Err(e) => return Err(e),
        //     }
        // }
        // Err(transport::Error::Io)

        Ok(())
    }

    fn fpb_lock(&mut self) -> Result<(), transport::Error> {
        self.probe.write32(FP_CTRL, FPB_ENABLE | FPB_KEY)
    }

    fn fpb_unlock(&mut self) -> Result<(), transport::Error> {
        self.probe.write32(FP_CTRL, FPB_KEY)
    }

    pub fn set_breakpoint(&mut self, addr: u32) -> Result<(), transport::Error> {
        let slot = self.breakpoints_slots.iter().position(|&a| a.is_none())
            .ok_or(transport::Error::Io)?;

        if slot >= self.breakpoints_num {
            return Err(transport::Error::Io); // TODO: define more specific error type
        }

        let comp = FP_COMP0 + (slot as u32) * 4;
        let replace = if (addr & 0x2) == 0 {
            FPB_BP_LOWER
        } else {
            FPB_BP_UPPER
        };
        let value = replace | (addr & 0x1FFF_FFFC) | FPB_COMP_ENABLE;

        self.fpb_unlock()?;
        self.probe.write32(comp, value)?;
        self.fpb_lock()?;

        let ctrl  = self.probe.read32(FP_CTRL)?;
        let back  = self.probe.read32(comp)?;
        println!("FP_CTRL  = 0x{:08X}", ctrl);
        println!("FP_COMP{} = 0x{:08X}", slot, back);
        println!("expected  0x{:08X}", value);

        self.breakpoints_slots[slot] = Some(addr);
        Ok(())
    }

    pub fn remove_breakpoint(&mut self, addr: u32) -> Result<(), transport::Error> {
        let slot = match self.breakpoints_slots.iter().position(|&a| a == Some(addr)) {
            Some(s) => s,
            None => return Ok(())
        };

        let comp = FP_COMP0 + (slot as u32) * 4;

        self.fpb_unlock()?;
        self.probe.write32(comp, 0)?;
        self.fpb_lock()?;

        self.breakpoints_slots[slot] = None;
        Ok(())
    }

    pub fn clear_breakpoints(&mut self) -> Result<(), transport::Error> {
        self.fpb_unlock()?;
        for slot in 0..self.breakpoints_num {
            let comp = FP_COMP0 + (slot as u32) * 4;
            self.probe.write32(comp, 0)?;
        }
        self.fpb_lock()?;
        self.breakpoints_slots = [None; 6];
        Ok(())
    }
}