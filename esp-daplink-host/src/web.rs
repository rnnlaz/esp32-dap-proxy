//! 内置 Web 控制台：axum + 单页静态面板（零构建）。
//!
//! 端点：
//! - `GET /`                  仪表盘页面
//! - `GET /api/status`        指标 JSON（前端 2s 轮询）
//! - `GET /api/logs/history`  历史日志（环形缓冲）
//! - `GET /api/logs/stream`   实时日志 SSE 流

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::logs::{self, LogEvent};
use crate::metrics;

#[derive(Clone)]
struct WebState {
    target: String,
    usbip_listen: String,
}

#[derive(Serialize)]
struct Status {
    uptime_secs: u64,
    listen_usbip: String,
    target: String,
    counters: Counters,
    rtt: RttStats,
    last_link_error: Option<String>,
}

#[derive(Serialize)]
struct RttStats {
    /// EWMA 平均（毫秒）
    avg_ms: f64,
    last_ms: f64,
    max_ms: f64,
}

#[derive(Serialize)]
struct Counters {
    usb_sessions: u64,
    ep0_requests: u64,
    ep1_out_frames: u64,
    ep1_out_bytes: u64,
    ep1_in_frames: u64,
    ep1_in_bytes: u64,
    link_requests: u64,
    link_retries: u64,
    link_errors: u64,
}

fn rtt_us(a: &std::sync::atomic::AtomicU32) -> f64 {
    a.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1000.0
}

fn rtt_stats() -> RttStats {
    RttStats {
        avg_ms: rtt_us(&metrics::LINK_RTT_US_EWMA),
        last_ms: rtt_us(&metrics::LINK_RTT_US_LAST),
        max_ms: rtt_us(&metrics::LINK_RTT_US_MAX),
    }
}

pub async fn serve(
    addr: &str,
    target: String,
    usbip_listen: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/api/status", get(status))
        .route("/api/logs/history", get(log_history))
        .route("/api/logs/stream", get(log_stream))
        .with_state(WebState {
            target,
            usbip_listen,
        });

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("🖥 Web 控制台: http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn dashboard() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
}

async fn status(State(st): State<WebState>) -> Json<Status> {
    Json(Status {
        uptime_secs: metrics::uptime_secs(),
        listen_usbip: st.usbip_listen,
        target: st.target,
        counters: Counters {
            usb_sessions: metrics::USB_SESSIONS.load(Ordering::Relaxed),
            ep0_requests: metrics::EP0_REQUESTS.load(Ordering::Relaxed),
            ep1_out_frames: metrics::EP1_OUT_FRAMES.load(Ordering::Relaxed),
            ep1_out_bytes: metrics::EP1_OUT_BYTES.load(Ordering::Relaxed),
            ep1_in_frames: metrics::EP1_IN_FRAMES.load(Ordering::Relaxed),
            ep1_in_bytes: metrics::EP1_IN_BYTES.load(Ordering::Relaxed),
            link_requests: metrics::LINK_REQUESTS.load(Ordering::Relaxed),
            link_retries: metrics::LINK_RETRIES.load(Ordering::Relaxed),
            link_errors: metrics::LINK_ERRORS.load(Ordering::Relaxed),
        },
        rtt: rtt_stats(),
        last_link_error: metrics::last_link_error(),
    })
}

async fn log_history() -> Json<Vec<Arc<LogEvent>>> {
    Json(logs::history())
}

async fn log_stream() -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = logs::subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| {
        item.ok().map(|ev| {
            let data = serde_json::to_string(&*ev).unwrap_or_default();
            Ok::<_, Infallible>(Event::default().event("log").data(data))
        })
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
