use esp_println::println;

use crate::channel::Channel;
use crate::probe::target::dp::{
    CDBGPWRUPACK, CDBGPWRUPREQ, CSYSPWRUPACK, CSYSPWRUPREQ, DP_ABORT, DP_CTRL_STAT, DP_DPIDR,
};
use crate::probe::transport::{Error as TErr, Transport};

const ID_INFO: u8 = 0x00;
const ID_HOST_STATUS: u8 = 0x01;
const ID_CONNECT: u8 = 0x02;
const ID_DISCONNECT: u8 = 0x03;
const ID_TRANSFER_CONFIG: u8 = 0x04;
const ID_TRANSFER: u8 = 0x05;
const ID_TRANSFER_BLOCK: u8 = 0x06;
const ID_WRITE_ABORT: u8 = 0x08;
const ID_SWJ_PINS: u8 = 0x10;
const ID_SWJ_CLOCK: u8 = 0x11;
const ID_SWJ_SEQUENCE: u8 = 0x12;
const ID_SWD_CONFIGURE: u8 = 0x13;

const ST_OK: u8 = 0x01;
const ST_WAIT: u8 = 0x02;
const ST_FAULT: u8 = 0x04;
const ST_PROTO: u8 = 0x08;

pub struct Dap<'a, T: Transport> {
    transport: &'a mut T,
    wait_retry: u16,
    err_logs: u32,
}

impl<'a, T: Transport> Dap<'a, T> {
    pub fn new(transport: &'a mut T) -> Self {
        Self { transport, wait_retry: 64, err_logs: 0 }
    }

    pub fn handle(&mut self, req: &[u8], resp: &mut [u8]) -> usize {
        if req.is_empty() {
            resp[0] = 0xFF;
            return 1;
        }
        resp[0] = req[0];
        match req[0] {
            ID_INFO => 1 + self.dap_info(req, &mut resp[1..]),
            ID_CONNECT => 1 + self.dap_connect(&mut resp[1..]),
            ID_DISCONNECT => {
                resp[1] = 0x00;
                2
            }
            ID_HOST_STATUS => {
                // DAP_HostStatus：接受连接/运行状态通知，直接 ACK
                resp[1] = 0x00;
                2
            }
            ID_TRANSFER_CONFIG => 1 + self.dap_transfer_config(req, &mut resp[1..]),
            ID_TRANSFER => 1 + self.dap_transfer(req, &mut resp[1..]),
            ID_TRANSFER_BLOCK => 1 + self.dap_transfer_block(req, &mut resp[1..]),
            ID_WRITE_ABORT => 1 + self.dap_write_abort(req, &mut resp[1..]),
            ID_SWJ_PINS => 1 + self.dap_swj_pins(req, &mut resp[1..]),
            ID_SWJ_CLOCK => 1 + self.dap_swj_clock(&mut resp[1..]),
            ID_SWD_CONFIGURE => {
                // DAP_SWD_Configure：位带实现无需配置，直接 ACK
                resp[1] = 0x00;
                2
            }
            ID_SWJ_SEQUENCE => 1 + self.dap_swj_sequence(req, &mut resp[1..]),
            _ => {
                resp[1] = 0xFF; // DAP_Error
                2
            }
        }
    }

    fn dap_info(&mut self, req: &[u8], resp: &mut [u8]) -> usize {
        let id = if req.len() > 1 { req[1] } else { 0 };
        let s: &[u8] = match id {
            0x01 => b"esp",
            0x02 => b"my-esp-debugger",
            0x04 => b"1.2.0",
            _ => b"",
        };
        if !s.is_empty() {
            resp[0] = s.len() as u8;
            resp[1..=s.len()].copy_from_slice(s);
            return 1 + s.len();
        }
        match id {
            // 目前仅支持 SWD
            0xF0 => {
                resp[0] = 1;
                resp[1] = 0x01;
                return 2;
            }

            0xFE => {
                resp[0] = 1;
                resp[1] = 1;
                return 2;
            }

            0xFF => {
                resp[0] = 2;
                resp[1] = 0x00;
                resp[2] = 0x02;
                return 3;
            }

            _ => {
                resp[0] = 0;
                return 1;
            }
        }
    }

    fn dap_connect(&mut self, resp: &mut [u8]) -> usize {
        self.transport.init().ok();

        match self.transport.read_dp(DP_DPIDR) {
            Ok(id) => println!("[dap] DPIDR = 0x{:08X}", id),
            Err(e) => println!("[dap] DPIDR read err: {:?}", e),
        }

        match self.transport.write_dp(DP_CTRL_STAT, CDBGPWRUPREQ | CSYSPWRUPREQ) {
            Ok(()) => println!("[dap] power req written"),
            Err(e) => println!("[dap] power req err: {:?}", e),
        }

        for i in 0..200 {
            match self.transport.read_dp(DP_CTRL_STAT) {
                Ok(stat) => {
                    if i < 3 {
                        println!("[dap] CTRL/STAT = 0x{:08X}", stat);
                    }
                    if stat & (CDBGPWRUPACK | CSYSPWRUPACK) == (CDBGPWRUPACK | CSYSPWRUPACK) {
                        resp[0] = 0x01;
                        return 1;
                    }
                }
                Err(e) => {
                    if i < 3 {
                        println!("[dap] read err: {:?}", e);
                    }
                }
            }
        }
        resp[0] = 0x00;
        1
    }

    fn dap_transfer_config(&mut self, req: &[u8], resp: &mut [u8]) -> usize {
        if req.len() >= 6 {
            self.wait_retry = u16::from_le_bytes([req[2], req[3]]);
        }
        resp[0] = 0x00;
        1
    }

    fn dap_write_abort(&mut self, req: &[u8], resp: &mut [u8]) -> usize {
        if req.len() >= 6 {
            let word = u32::from_le_bytes([req[2], req[3], req[4], req[5]]);
            let _ = self.transport.write_dp(DP_ABORT, word);
        }
        resp[0] = 0x00;
        1
    }

    fn dap_transfer(&mut self, req: &[u8], resp: &mut [u8]) -> usize {
        if req.len() < 3 {
            resp[0] = 0;
            resp[1] = ST_PROTO;
            return 2;
        }
        let count = req[2] as usize;
        let mut pos = 3;
        let mut rpos = 2;
        let mut done = 0u8;
        let mut status = ST_OK;

        for _ in 0..count {
            if pos >= req.len() {
                status = ST_PROTO;
                break;
            }
            let rb = req[pos];
            pos += 1;

            let apndp = rb & 0x01 != 0;
            let rnw = rb & 0x02 != 0;
            let a = ((rb >> 2) & 0x03) << 2;
            if rb & 0xF0 != 0 {
                status = ST_PROTO;
                break;
            }

            let wdata = if !rnw {
                if pos + 4 > req.len() {
                    status = ST_PROTO;
                    break;
                }
                let v = u32::from_le_bytes(req[pos..pos + 4].try_into().unwrap());
                pos += 4;
                Some(v)
            } else {
                None
            };

            match self.exec_transfer(apndp, rnw, a, wdata) {
                Ok(Some(v)) => {
                    resp[rpos..rpos + 4].copy_from_slice(&v.to_le_bytes());
                    rpos += 4;
                    done += 1;
                }
                Ok(None) => done += 1,
                Err(s) => {
                    status = s;
                    break;
                }
            }
        }

        resp[0] = done;
        resp[1] = status;
        rpos
    }

    fn exec_transfer(&mut self, apndp: bool, rnw: bool, a: u8, wdata: Option<u32>) -> Result<Option<u32>, u8> {
        let mut tries = 0u32;
        loop {
            let r = match (apndp, rnw) {
                (false, true) => self.transport.read_dp(a).map(Some),
                (false, false) => self.transport.write_dp(a, wdata.unwrap_or(0)).map(|_| None),
                (true, true) => self.transport.read_ap(0, a).map(Some),
                (true, false) => self.transport.write_ap(0, a, wdata.unwrap_or(0)).map(|_| None),
            };
            let status = match r {
                Ok(v) => return Ok(v),
                Err(TErr::Wait) => ST_WAIT,
                Err(TErr::Fault) => ST_FAULT,

                Err(TErr::Parity) => {
                    self.log_xfer_err("parity-error", a, apndp, rnw, tries);
                    ST_PROTO
                }
                Err(e) => {
                    self.log_xfer_err("no-ack/garbage-ack", a, apndp, rnw, tries);
                    let _ = e;
                    ST_PROTO
                }
            };
            match status {
                // WAIT 在固件端重试；超出封顶后交还宿主处理
                ST_WAIT if (tries as u16) < self.wait_retry.min(128) => tries += 1,
                _ => return Err(status),
            }
        }
    }

    fn log_xfer_err(&mut self, kind: &str, a: u8, apndp: bool, rnw: bool, tries: u32) {
        if self.err_logs < 8 {
            self.err_logs += 1;
            println!(
                "[dap] xfer {}: addr=0x{:02X} apndp={} rnw={} tries={}",
                kind, a, apndp as u8, rnw as u8, tries
            );
        }
    }

    fn dap_transfer_block(&mut self, req: &[u8], resp: &mut [u8]) -> usize {
        if req.len() < 5 {
            resp[..3].copy_from_slice(&[0, 0, ST_PROTO]);
            return 3;
        }
        let count = (u16::from_le_bytes([req[2], req[3]]) as usize).min((resp.len() - 3) / 4);
        let rb = req[4];
        let apndp = rb & 0x01 != 0;
        let rnw = rb & 0x02 != 0;
        let a = ((rb >> 2) & 0x03) << 2;
        if rb & 0xF0 != 0 {
            resp[..3].copy_from_slice(&[0, 0, ST_PROTO]);
            return 3;
        }

        let mut pos = 5;
        let mut rpos = 3;
        let mut done = 0u16;
        let mut status = ST_OK;

        for _ in 0..count {
            let wdata = if !rnw {
                if pos + 4 > req.len() {
                    status = ST_PROTO;
                    break;
                }
                let v = u32::from_le_bytes(req[pos..pos + 4].try_into().unwrap());
                pos += 4;
                Some(v)
            } else {
                None
            };

            match self.exec_transfer(apndp, rnw, a, wdata) {
                Ok(Some(v)) => {
                    resp[rpos..rpos + 4].copy_from_slice(&v.to_le_bytes());
                    rpos += 4;
                    done += 1;
                }
                Ok(None) => done += 1,
                Err(s) => {
                    status = s;
                    break;
                }
            }
        }

        resp[0..2].copy_from_slice(&done.to_le_bytes());
        resp[2] = status;
        rpos
    }

    fn dap_swj_clock(&mut self, resp: &mut [u8]) -> usize {
        // 目前由固件延时决定
        resp[0] = 0x00;
        1
    }

    fn dap_swj_sequence(&mut self, req: &[u8], resp: &mut [u8]) -> usize {
        if req.len() < 2 {
            resp[0] = 0xFF;
            return 1;
        }
        let bits = req[1] as usize;
        let nbytes = bits.div_ceil(8);
        if req.len() < 2 + nbytes {
            resp[0] = 0xFF;
            return 1;
        }
        let _ = self.transport.swj_sequence(req[1], &req[2..2 + nbytes]);
        resp[0] = 0x00;
        1
    }

    fn dap_swj_pins(&mut self, req: &[u8], resp: &mut [u8]) -> usize {
        if req.len() < 10 {
            resp[0] = 0xFF;
            return 1;
        }
        let wait = u32::from_le_bytes([req[6], req[7], req[8], req[9]]);
        resp[0] = self.transport.swj_pins(req[1], req[2], wait).unwrap_or(0x00);
        1
    }
}

pub async fn serve<C: Channel, T: Transport>(ch: &mut C, transport: &mut T) {
    let mut dap = Dap::new(transport);
    let mut frame = [0u8; 1024];
    let mut resp = [0u8; 1024];
    loop {
        let n = match ch.recv_frame(&mut frame).await {
            Ok(n) => n,
            Err(e) => {
                println!("[dap] session end: {:?}", e);
                return;
            }
        };
        let rn = dap.handle(&frame[..n], &mut resp);
        if ch.send_frame(&resp[..rn]).await.is_err() {
            return;
        }
    }
}
