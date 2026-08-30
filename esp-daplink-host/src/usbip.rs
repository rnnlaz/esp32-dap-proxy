use std::error::Error;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::bridge::DapBridge;
use crate::descriptors;
use crate::metrics;

const VERSION: u16 = 0x0111;

const OP_REQ_DEVLIST: u16 = 0x8005;
const OP_REP_DEVLIST: u16 = 0x0005;
const OP_REQ_IMPORT: u16 = 0x8003;
const OP_REP_IMPORT: u16 = 0x0003;

const CMD_SUBMIT: u32 = 0x0001;
const CMD_UNLINK: u32 = 0x0002;
const RET_SUBMIT: u32 = 0x0003;
const RET_UNLINK: u32 = 0x0004;

const DIR_OUT: u32 = 0;
const DIR_IN: u32 = 1;

const EIO: i32 = 5;
const EPIPE: i32 = 32;

/// CMD_SUBMIT / CMD_UNLINK / RET_SUBMIT / RET_UNLINK 通用头长
const HEADER_LEN: usize = 48;

pub async fn serve(addr: &str, target: &str) -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(
        "等待 USB/IP 客户端 attach（busid {}）…",
        descriptors::USBIP_BUSID
    );
    loop {
        let (stream, peer) = listener.accept().await?;
        let target = target.to_string();
        tracing::info!("客户节点连入: {peer}");
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, target).await {
                tracing::error!("连接异常中断: {e}");
            }
            tracing::info!("客户节点断开: {peer}");
        });
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    target: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut head = [0u8; 8];
    loop {
        if stream.read_exact(&mut head).await.is_err() {
            return Ok(());
        }
        let version = u16::from_be_bytes([head[0], head[1]]);
        let command = u16::from_be_bytes([head[2], head[3]]);

        if version != VERSION {
            tracing::warn!("协议版本不匹配: 0x{version:04x}");
            return Ok(());
        }

        match command {
            OP_REQ_DEVLIST => reply_devlist(&mut stream).await?,
            OP_REQ_IMPORT => {
                if !reply_import(&mut stream).await? {
                    return Ok(());
                }
                return urb_loop(&mut stream, DapBridge::new(target)).await;
            }
            _ => {
                tracing::warn!("未知请求命令字: 0x{command:04x}");
                return Ok(());
            }
        }
    }
}

async fn reply_devlist(stream: &mut TcpStream) -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing::info!("OP_REQ_DEVLIST → 导出虚拟 CMSIS-DAP v2");
    let mut r = Vec::with_capacity(0x148);
    r.extend_from_slice(&VERSION.to_be_bytes());
    r.extend_from_slice(&OP_REP_DEVLIST.to_be_bytes());
    r.extend_from_slice(&0u32.to_be_bytes()); // status
    r.extend_from_slice(&1u32.to_be_bytes()); // 导出设备数
    push_device_body(&mut r);
    r.extend_from_slice(&[0xFF, 0x00, 0x00, 0x00]);
    stream.write_all(&r).await?;
    stream.flush().await?;
    Ok(())
}

async fn reply_import(stream: &mut TcpStream) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let mut busid = [0u8; 32];
    stream.read_exact(&mut busid).await?;
    let busid = String::from_utf8_lossy(&busid);
    let busid = busid.trim_end_matches('\0');

    let mut r = Vec::with_capacity(8 + 0x140);
    r.extend_from_slice(&VERSION.to_be_bytes());
    r.extend_from_slice(&OP_REP_IMPORT.to_be_bytes());

    if busid != descriptors::USBIP_BUSID {
        tracing::warn!(
            "OP_REQ_IMPORT 目标 busid 不匹配: {busid:?}（期望 {:?}）",
            descriptors::USBIP_BUSID
        );
        r.extend_from_slice(&1u32.to_be_bytes());
        stream.write_all(&r).await?;
        return Ok(false);
    }

    tracing::info!("OP_REQ_IMPORT → 握手成功，进入 URB 循环");
    metrics::bump(&metrics::USB_SESSIONS);
    r.extend_from_slice(&0u32.to_be_bytes());
    push_device_body(&mut r);
    stream.write_all(&r).await?;
    stream.flush().await?;
    Ok(true)
}

fn push_device_body(buf: &mut Vec<u8>) {
    let mut path = [0u8; 256];
    path[..descriptors::USBIP_PATH.len()].copy_from_slice(descriptors::USBIP_PATH.as_bytes());
    buf.extend_from_slice(&path);

    let mut busid = [0u8; 32];
    busid[..descriptors::USBIP_BUSID.len()].copy_from_slice(descriptors::USBIP_BUSID.as_bytes());
    buf.extend_from_slice(&busid);

    buf.extend_from_slice(&1u32.to_be_bytes()); // busnum
    buf.extend_from_slice(&1u32.to_be_bytes()); // devnum
    buf.extend_from_slice(&3u32.to_be_bytes()); // speed: high
    buf.extend_from_slice(&descriptors::VID.to_be_bytes());
    buf.extend_from_slice(&descriptors::PID.to_be_bytes());
    buf.extend_from_slice(&descriptors::BCD_DEVICE.to_be_bytes());
    buf.push(0x00); // bDeviceClass
    buf.push(0x00); // bDeviceSubClass
    buf.push(0x00); // bDeviceProtocol
    buf.push(0x01); // bConfigurationValue
    buf.push(0x01); // bNumConfigurations
    buf.push(0x01); // bNumInterfaces
}

async fn urb_loop(
    stream: &mut TcpStream,
    mut bridge: DapBridge,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut hdr = [0u8; HEADER_LEN];
    loop {
        if stream.read_exact(&mut hdr).await.is_err() {
            return Ok(());
        }
        let command = u32::from_be_bytes(hdr[0..4].try_into().unwrap());
        let seqnum = u32::from_be_bytes(hdr[4..8].try_into().unwrap());
        let direction = u32::from_be_bytes(hdr[12..16].try_into().unwrap());
        let ep = u32::from_be_bytes(hdr[16..20].try_into().unwrap());
        let transfer_len = u32::from_be_bytes(hdr[24..28].try_into().unwrap());

        match command {
            CMD_SUBMIT => {
                // 先读保持流同步
                let mut out_data = Vec::new();
                if direction == DIR_OUT && transfer_len > 0 {
                    out_data.resize(transfer_len as usize, 0);
                    if stream.read_exact(&mut out_data).await.is_err() {
                        return Ok(());
                    }
                }

                let (status, in_data) = match (ep, direction) {
                    (0, _) => {
                        metrics::bump(&metrics::EP0_REQUESTS);
                        ep0_control(&hdr[40..48], &out_data)
                    }
                    (_, DIR_OUT) => {
                        tracing::debug!(
                            "EP1 OUT ← DAP 命令 0x{:02X}（{} 字节）",
                            out_data.first().copied().unwrap_or(0),
                            out_data.len()
                        );
                        match bridge.bulk_out(&out_data).await {
                            Ok(()) => {
                                metrics::bump(&metrics::EP1_OUT_FRAMES);
                                metrics::add(&metrics::EP1_OUT_BYTES, out_data.len() as u64);
                                (0, Vec::new())
                            }
                            Err(e) => {
                                tracing::warn!("EP1 OUT 转发失败: {e}");
                                (EIO, Vec::new())
                            }
                        }
                    }
                    (_, DIR_IN) => match bridge.bulk_in(transfer_len as usize).await {
                        Ok(data) => {
                            metrics::bump(&metrics::EP1_IN_FRAMES);
                            metrics::add(&metrics::EP1_IN_BYTES, data.len() as u64);
                            tracing::debug!(
                                "EP1 IN → DAP 响应 0x{:02X}({} 字节){}",
                                data.first().copied().unwrap_or(0),
                                data.len(),
                                describe_dap_status(&data)
                            );
                            (0, data)
                        }
                        Err(e) => {
                            tracing::warn!("EP1 IN 转发失败: {e}");
                            (EIO, Vec::new())
                        }
                    },
                    _ => (EPIPE, Vec::new()),
                };
                send_ret_submit(stream, seqnum, status, &in_data).await?;
            }
            CMD_UNLINK => {
                let unlink_seqnum = u32::from_be_bytes(hdr[20..24].try_into().unwrap());
                tracing::debug!("CMD_UNLINK: unlink_seqnum={unlink_seqnum}");
                send_ret_unlink(stream, seqnum).await?;
            }
            _ => {
                tracing::warn!("未知 URB 命令 0x{command:08x}，断开会话");
                return Ok(());
            }
        }
    }
}

fn ep0_control(setup: &[u8], _out_data: &[u8]) -> (i32, Vec<u8>) {
    let bm_request_type = setup[0];
    let b_request = setup[1];
    let w_value = u16::from_le_bytes([setup[2], setup[3]]);
    let w_length = u16::from_le_bytes([setup[6], setup[7]]);

    if bm_request_type & 0x80 != 0 {
        let payload: Option<Vec<u8>> =
            match (bm_request_type, b_request, w_value >> 8, w_value & 0xFF) {
                (0x80, 0x00, _, _) => Some(vec![0x00, 0x00]), // GET_STATUS
                (0x80, 0x06, 0x01, _) => Some(descriptors::DEVICE_DESCRIPTOR.to_vec()),
                (0x80, 0x06, 0x02, _) => Some(descriptors::CONFIG_DESCRIPTOR.to_vec()),
                (0x80, 0x06, 0x03, index) => descriptors::string_descriptor(index as u8),
                (0x80, 0x08, _, _) => Some(vec![0x01]), // GET_CONFIGURATION
                (0x81, 0x00, _, _) | (0x82, 0x00, _, _) => Some(vec![0x00, 0x00]), // GET_STATUS(interface/endpoint)
                (0x80, 0x0A, _, _) => Some(vec![0x00]),                            // GET_INTERFACE
                _ => None,
            };
        return match payload {
            Some(p) => {
                let n = p.len().min(w_length as usize);
                (0, p[..n].to_vec())
            }
            None => (EPIPE, Vec::new()),
        };
    }

    match b_request {
        0x01 | 0x03 | 0x09 | 0x0B => (0, Vec::new()),
        _ => (EPIPE, Vec::new()),
    }
}

fn describe_dap_status(data: &[u8]) -> String {
    match data.first() {
        Some(0x05) if data.len() >= 3 => format!(" count={} status=0x{:02X}", data[1], data[2]),
        Some(0x06) if data.len() >= 4 => {
            let count = u16::from_le_bytes([data[1], data[2]]);
            format!(" count={} status=0x{:02X}", count, data[3])
        }
        _ => String::new(),
    }
}

async fn send_ret_submit(
    stream: &mut TcpStream,
    seqnum: u32,
    status: i32,
    data: &[u8],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut r = Vec::with_capacity(HEADER_LEN + data.len());
    r.extend_from_slice(&RET_SUBMIT.to_be_bytes());
    r.extend_from_slice(&seqnum.to_be_bytes());
    r.extend_from_slice(&0u32.to_be_bytes()); // devid
    r.extend_from_slice(&0u32.to_be_bytes()); // direction
    r.extend_from_slice(&0u32.to_be_bytes()); // ep
    r.extend_from_slice(&status.to_be_bytes());
    r.extend_from_slice(&(data.len() as u32).to_be_bytes()); // actual_length
    r.extend_from_slice(&0u32.to_be_bytes()); // start_frame
    r.extend_from_slice(&0u32.to_be_bytes()); // number_of_packets
    r.extend_from_slice(&0u32.to_be_bytes()); // error_count
    r.extend_from_slice(&[0u8; 8]); // padding
    r.extend_from_slice(data); // IN 方向载荷
    stream.write_all(&r).await?;
    stream.flush().await?;
    Ok(())
}

async fn send_ret_unlink(
    stream: &mut TcpStream,
    seqnum: u32,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut r = Vec::with_capacity(HEADER_LEN);
    r.extend_from_slice(&RET_UNLINK.to_be_bytes());
    r.extend_from_slice(&seqnum.to_be_bytes());
    r.extend_from_slice(&0u32.to_be_bytes()); // devid
    r.extend_from_slice(&0u32.to_be_bytes()); // direction
    r.extend_from_slice(&0u32.to_be_bytes()); // ep
    r.extend_from_slice(&0i32.to_be_bytes()); // status
    r.extend_from_slice(&[0u8; 24]); // padding
    stream.write_all(&r).await?;
    stream.flush().await?;
    Ok(())
}
