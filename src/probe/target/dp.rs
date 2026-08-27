pub const DP_DPIDR:     u8 = 0x00;
pub const DP_CTRL_STAT: u8 = 0x04;
pub const DP_SELECT:    u8 = 0x08;
pub const DP_RDBUFF:    u8 = 0x0C;

pub const DP_ABORT:     u8 = 0x00;

// CTRL/STAT
pub const CDBGPWRUPREQ: u32 = 1 << 28;
pub const CDBGPWRUPACK: u32 = 1 << 29;
pub const CSYSPWRUPREQ: u32 = 1 << 30;
pub const CSYSPWRUPACK: u32 = 1 << 31;

pub const ORUNDETECT:   u32 = 1 << 0;
pub const STICKYORUN:   u32 = 1 << 1;
pub const STICKYCMP:    u32 = 1 << 4;
pub const STICKYERR:    u32 = 1 << 5;
pub const READOK:       u32 = 1 << 6;
pub const WDATAERR:     u32 = 1 << 7;

// SELECT
pub const SELECT_APBANKSEL_MASK: u32 = 0x0000_00F0;
pub const SELECT_APSEL_MASK:     u32 = 0xFF00_0000;

// ABORT
pub const DAPABORT:     u32 = 1 << 0;
pub const STKCMPCLR:    u32 = 1 << 1;
pub const STKERRCLR:    u32 = 1 << 2;
pub const WDERRCLR:     u32 = 1 << 3;
pub const ORUNERRCLR:   u32 = 1 << 4;