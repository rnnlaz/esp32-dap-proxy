ub const USBIP_PATH: &str = "/sys/v_cmsisdap";
/// 总线 ID（USB/IP attach 时的 busid）
pub const USBIP_BUSID: &str = "1-1";

pub const VID: u16 = 0xC251;
pub const PID: u16 = 0xF001; // CMSIS-DAP
pub const BCD_DEVICE: u16 = 0x0200;

pub const VENDOR: &str = "ARM";
pub const PRODUCT: &str = "CMSIS-DAP v2";
pub const SERIAL: &str = "0001A0000001";
pub const INTERFACE: &str = "CMSIS-DAP v2";

/// 设备描述符（18 字节）
pub const DEVICE_DESCRIPTOR: &[u8] = &[
    18, // bLength
    0x01, // bDescriptorType: DEVICE
    0x00, 0x02, // bcdUSB: 2.00
    0x00, 0x00, 0x00, // class / subclass / protocol: 在接口级声明
    64, // bMaxPacketSize0
    (VID & 0xFF) as u8, (VID >> 8) as u8,
    (PID & 0xFF) as u8, (PID >> 8) as u8,
    (BCD_DEVICE & 0xFF) as u8, (BCD_DEVICE >> 8) as u8,
    1, 2, 3, // iManufacturer / iProduct / iSerialNumber
    1, // bNumConfigurations
];

/// 配置描述符（34 字节）：config + interface + EP1 OUT bulk + EP1 IN bulk
pub const CONFIG_DESCRIPTOR: &[u8] = &[
    // Config Header
    9, 0x02, 32, 0x00, 1, 1, 0, 0x80, 250,
    // Interface Descriptor - Vendor Specific（CMSIS-DAP v2）
    9, 0x04, 0, 0, 2, 0xFF, 0x00, 0x00, 4,
    // EP1 OUT Bulk, wMaxPacketSize = 512
    7, 0x05, 0x01, 0x02, 0x00, 0x02, 0,
    // EP1 IN Bulk, wMaxPacketSize = 512
    7, 0x05, 0x81, 0x02, 0x00, 0x02, 0,
];

/// 语言 ID 描述符（GET_DESCRIPTOR(string), wIndex=0）
pub const LANGID_DESCRIPTOR: &[u8] = &[4, 0x03, 0x09, 0x04];

/// 按字符串索引返回 UTF-16LE 字符串描述符。
/// 索引与设备描述符的 iManufacturer/iProduct/iSerialNumber 对应。
pub fn string_descriptor(index: u8) -> Option<Vec<u8>> {
    let s = match index {
        0 => return Some(LANGID_DESCRIPTOR.to_vec()),
        1 => VENDOR,
        2 => PRODUCT,
        3 => SERIAL,
        4 => INTERFACE,
        _ => return None,
    };
    let mut out = Vec::with_capacity(2 + s.len() * 2);
    out.push((2 + s.len() as u16 * 2) as u8);
    out.push(0x03); // bDescriptorType: STRING
    for unit in s.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    Some(out)
}
