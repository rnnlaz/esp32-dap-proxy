use esp_println::println;

use crate::host::Channel;
use crate::probe::target::dp::{
    CDBGPWRUPACK, CDBGPWRUPREQ, CSYSPWRUPACK, CSYSPWRUPREQ, DP_ABORT, DP_CTRL_STAT, DP_DPIDR,
};
use crate::probe::transport::{Error as TErr, Transport};

const ID_INFO: u8 = 0x00;
const ID_CONNECT: u8 = 0x02;
const ID_DISCONNECT: u8 = 0x03;
const ID_TRANSFER_CONFIG: u8 = 0x04;
const ID_TRANSFER: u8 = 0x05;
const ID_WRITE_ABORT: u8 = 0x08;

const ST_OK: u8 = 0x01;
const ST_WAIT: u8 = 0x02;
const ST_FAULT: u8 = 0x04;
const ST_PROTO: u8 = 0x08;

pub struct Dap<'a, T: Transport> {
    transport: &'a mut T,
    wait_retry: u16,
}

impl<'a, T: Transport> Dap<'a, T> {
    pub fn new(transport: &'a mut T) -> Self {
        Self { transport, wait_retry: 64 }
    }

    pub fn handle(&mut self, req: &[u8], resp: &mut [u8]) -> usize {
        if req.is_empty() {
            resp[0] = 0xFF;
            return 1;
        }
        match req[0] {
            ID_INFO => self.dap_info(req, resp),
            ID_CONNECT => self.dap_connect(resp),
            ID_DISCONNECT => {
                resp[0] = 0x00;
                1
            }
            ID_TRANSFER_CONFIG => self.dap_transfer_config(req, resp),
            ID_TRANSFER => self.dap_transfer(req, resp),
            ID_WRITE_ABORT => self.dap_write_abort(req, resp),
            _ => {
                resp[0] = 0xFF;
                1
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
        if id == 0xF0 {
            resp[0] = 1;
            resp[1] = 0x01;
            return 2;
        }
        resp[0] = 0;
        1
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

        for i in 0..1000 {
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

            let mut tries = 0u32;
            let result = loop {
                match self.one_transfer(apndp, rnw, a, wdata) {
                    Err(ST_WAIT) if (tries as u16) < self.wait_retry => tries += 1,
                    other => break other,
                }
            };

            match result {
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

    fn one_transfer(&mut self, apndp: bool, rnw: bool, a: u8, wdata: Option<u32>) -> Result<Option<u32>, u8> {
        let r = match (apndp, rnw) {
            (false, true) => self.transport.read_dp(a).map(Some),
            (false, false) => self.transport.write_dp(a, wdata.unwrap_or(0)).map(|_| None),
            (true, true) => self.transport.read_ap(0, a).map(Some),
            (true, false) => self.transport.write_ap(0, a, wdata.unwrap_or(0)).map(|_| None),
        };
        r.map_err(|e| match e {
            TErr::Wait => ST_WAIT,
            TErr::Fault => ST_FAULT,
            _ => ST_PROTO,
        })
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
