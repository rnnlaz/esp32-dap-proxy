use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub static USB_SESSIONS: AtomicU64 = AtomicU64::new(0);
pub static EP0_REQUESTS: AtomicU64 = AtomicU64::new(0);
pub static EP1_OUT_FRAMES: AtomicU64 = AtomicU64::new(0);
pub static EP1_OUT_BYTES: AtomicU64 = AtomicU64::new(0);
pub static EP1_IN_FRAMES: AtomicU64 = AtomicU64::new(0);
pub static EP1_IN_BYTES: AtomicU64 = AtomicU64::new(0);
pub static LINK_REQUESTS: AtomicU64 = AtomicU64::new(0);
pub static LINK_RETRIES: AtomicU64 = AtomicU64::new(0);
pub static LINK_ERRORS: AtomicU64 = AtomicU64::new(0);

/// 链路命令往返耗时（微秒）：last / EWMA 均值 / 历史最大。
/// 调试会话的断点延迟 ≈ 命令数 × RTT，这个数字直接定位瓶颈。
pub static LINK_RTT_US_LAST: AtomicU32 = AtomicU32::new(0);
pub static LINK_RTT_US_EWMA: AtomicU32 = AtomicU32::new(0);
pub static LINK_RTT_US_MAX: AtomicU32 = AtomicU32::new(0);

static START: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_LINK_ERROR: Mutex<Option<String>> = Mutex::new(None);

pub fn mark_start() {
    if let Ok(mut slot) = START.lock() {
        *slot = Some(Instant::now());
    }
}

pub fn uptime_secs() -> u64 {
    let Ok(guard) = START.lock() else { return 0 };
    guard.as_ref().map(|t| t.elapsed().as_secs()).unwrap_or(0)
}

/// 原子计数 +1
pub fn bump(c: &AtomicU64) {
    c.fetch_add(1, Ordering::Relaxed);
}

/// 原子计数 +n
pub fn add(c: &AtomicU64, n: u64) {
    c.fetch_add(n, Ordering::Relaxed);
}

/// 记录一次链路最终失败（重连重试后仍失败），供控制台展示。
pub fn note_link_error(msg: String) {
    bump(&LINK_ERRORS);
    if let Ok(mut slot) = LAST_LINK_ERROR.lock() {
        *slot = Some(msg);
    }
}

/// 记录一次成功的命令往返耗时（微秒）。
pub fn note_rtt(micros: u64) {
    let v = micros.min(u32::MAX as u64) as u32;
    LINK_RTT_US_LAST.store(v, Ordering::Relaxed);

    // EWMA：新样本权重 1/5
    let old = LINK_RTT_US_EWMA.load(Ordering::Relaxed) as u64;
    let avg = if old == 0 {
        v as u64
    } else {
        (old * 4 + v as u64) / 5
    };
    LINK_RTT_US_EWMA.store(avg.min(u32::MAX as u64) as u32, Ordering::Relaxed);

    // 历史最大（单调）
    let mut cur = LINK_RTT_US_MAX.load(Ordering::Relaxed);
    while v > cur {
        match LINK_RTT_US_MAX.compare_exchange_weak(cur, v, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(actual) => cur = actual,
        }
    }
}

pub fn last_link_error() -> Option<String> {
    LAST_LINK_ERROR.lock().ok().and_then(|s| s.clone())
}
