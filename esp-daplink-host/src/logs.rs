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
    pub seq: u64,
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
    let ts = START
        .get()
        .map(|t| t.elapsed().as_secs_f64())
        .unwrap_or(0.0);
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

pub fn subscribe() -> broadcast::Receiver<Arc<LogEvent>> {
    HUB.get()
        .expect("logs::init() must be called before subscribe()")
        .tx
        .subscribe()
}

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
            self.extra
                .push((field.name().to_string(), format!("{value:?}")));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.extra
                .push((field.name().to_string(), value.to_string()));
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
        publish(
            event.metadata().level().to_string().as_str(),
            event.metadata().target(),
            v.message,
        );
    }
}
