pub const AP_CSW:  u8 = 0x00;
pub const AP_TAR:  u8 = 0x04;
pub const AP_DRW:  u8 = 0x0C;
pub const AP_BD0:  u8 = 0x10;
pub const AP_BD1:  u8 = 0x14;
pub const AP_BD2:  u8 = 0x18;
pub const AP_BD3:  u8 = 0x1C;
pub const AP_CFG:  u8 = 0xF4;
pub const AP_BASE: u8 = 0xF8;
pub const AP_IDR:  u8 = 0xFC;

// CSW
pub const CSW_SIZE:      u32 = 0x0000_0007;
pub const CSW_SIZE_8:    u32 = 0;
pub const CSW_SIZE_16:   u32 = 1;
pub const CSW_SIZE_32:   u32 = 2;

pub const CSW_ADDRINC:      u32 = 0x0000_0030;
pub const CSW_ADDRINC_OFF:  u32 = 0 << 4;
pub const CSW_ADDRINC_SINGLE: u32 = 1 << 4;
pub const CSW_ADDRINC_PACKED: u32 = 2 << 4;

// STM32 Debug/CoreSight
pub const CSW_DEVICE_EN:     u32 = (1 << 29) | (1 << 6);
pub const CSW_EXTRA_BITS:    u32 = 1 << 25;
pub const CSW_DBGSWEN:       u32 = 1 << 31;

pub const CSW_32_OFF:    u32 = CSW_SIZE_32 | CSW_ADDRINC_OFF |
                               CSW_DBGSWEN | CSW_DEVICE_EN | CSW_EXTRA_BITS;
pub const CSW_32_SINGLE: u32 = CSW_SIZE_32 | CSW_ADDRINC_SINGLE |
                               CSW_DBGSWEN | CSW_DEVICE_EN | CSW_EXTRA_BITS;