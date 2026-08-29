//! 控制台日志中枢：截获 tracing 事件 → 环形缓冲（历史）+ 广播通道（实时 SSE）。
//!
//! `HubLayer` 挂在 tracing_subscriber 的 registry 上，与终端 fmt 输出并行，
//! 不改变原有日志行为。Web 控制台通过 `/api/logs/history` + `/api/logs/stream`
//! 消费。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

const RING_CAPACITY: usize = 512;
const CHANNEL_CAPACITY: usize = 1024;

#[derive(Serialize, Clone)]
pub struct LogEvent {
    /// 单调递增序号，前端用它去重（history + 实时流衔接处）
    pub seq: u64,
    /// 自进程启动起的秒数
    pub ts: f64,
    pub level: String,
    pub target: String,
    pub message: String,
}

struct Hub {
    tx: broadcast::Sender<Arc<LogEvent>>,
    ring: Mutex<VecDeque<Arc<LogEvent>>>,
}

static HUB: OnceLock<Hub> = OnceLock::new();
static SEQ: AtomicU64 = AtomicU64::new(0);
static START: OnceLock<std::time::Instant> = OnceLock::new();

/// 必须在 tracing 初始化之前调用。
pub fn init() {
    let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
    let _ = HUB.set(Hub {
        tx,
        ring: Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
    });
    let _ = START.set(std::time::Instant::now());
}

fn publish(level: &str, target: &str, message: String) {
    let Some(hub) = HUB.get() else { return };
    let ts = START.get().map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
    let ev = Arc::new(LogEvent {
        seq: SEQ.fetch_add(1, Ordering::Relaxed),
        ts,
        level: level.to_string(),
        target: target.to_string(),
        message,
    });
    if let Ok(mut ring) = hub.ring.lock() {
        if ring.len() >= RING_CAPACITY {
            ring.pop_front();
        }
        ring.push_back(ev.clone());
    }
    let _ = hub.tx.send(ev);
}

/// 启动以来的历史日志（最多 RING_CAPACITY 条）。
pub fn history() -> Vec<Arc<LogEvent>> {
    HUB.get()
        .and_then(|h| {
            h.ring
                .lock()
                .ok()
                .map(|ring| ring.iter().cloned().collect())
        })
        .unwrap_or_default()
}

/// 订阅实时日志流。
pub fn subscribe() -> broadcast::Receiver<Arc<LogEvent>> {
    HUB.get()
        .expect("logs::init() must be called before subscribe()")
        .tx
        .subscribe()
}

/// 把 tracing 事件克隆一份投递给 Web 控制台的 Layer。
pub struct HubLayer;

struct FieldCollector {
    message: String,
    extra: Vec<(String, String)>,
}

impl Visit for FieldCollector {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.extra.push((field.name().to_string(), format!("{value:?}")));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.extra.push((field.name().to_string(), value.to_string()));
        }
    }
}

impl<S: Subscriber> Layer<S> for HubLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut v = FieldCollector {
            message: String::new(),
            extra: Vec::new(),
        };
        event.record(&mut v);
        if !v.extra.is_empty() {
            v.message.push(' ');
            for (k, val) in &v.extra {
                v.message.push_str(&format!("{k}={val} "));
            }
            v.message.pop();
        }
        publish(event.metadata().level().to_string().as_str(), event.metadata().target(), v.message);
    }
}
