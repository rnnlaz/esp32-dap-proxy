//! esp-daplink-host
//!
//! 在 PC 上把 WiFi 另一端的 ESP32-C3 模拟成一个真实的 USB CMSIS-DAP v2 探头：
//!
//! ```text
//! OpenOCD / PyOCD / probe-rs
//!   │  USB/IP（内核虚拟 HCD，默认端口 3240）
//!   ▼
//! esp-daplink-host（本项目）
//!   │  0xDA 帧协议（WiFi TCP）
//!   ▼
//! esp-daplink-target（ESP32-C3）── bit-bang SWD ──▶ 被调试芯片
//! ```
//!
//! 模块划分：
//! - `usbip`       USB/IP 协议层：OP 握手 + URB 循环。纯协议，不感知 ESP32 细节
//! - `descriptors` 虚拟 USB 设备的描述符，devlist / import / EP0 共用的唯一事实来源
//! - `bridge`      桥接层：把 bulk EP1 语义（OUT=请求 / IN=响应）映射到链路
//! - `link`        与 ESP32 的 0xDA 帧协议链路：连接、重连、超时、keepalive
//! - `web`         内置控制台：仪表盘 + 实时日志（SSE）
//! - `logs`/`metrics` 日志中枢与指标计数器，供控制台消费
//!
//! 后续规划（架构已预留位置）：
//! - `scanner/`    ESP32 在线扫描（UDP 广播发现）
//! - `config`      ESP32 模式配置、目标地址簿

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod bridge;
mod descriptors;
mod link;
mod logs;
mod metrics;
mod usbip;
mod web;

/// 命令行参数（零依赖手写解析，未来并入 config 模块）。
#[derive(Debug, Clone)]
struct Args {
    /// USB/IP 监听地址
    listen: String,
    /// ESP32 DAP 通道地址（host:port）
    target: String,
    /// Web 控制台监听地址
    web: String,
}

const USAGE: &str = "\
esp-daplink-host — 虚拟 USB CMSIS-DAP v2 探头（USB/IP 服务器）

用法:
  esp-daplink-host [--listen <addr>] [--target <host:port>] [--web <addr>]

选项:
  --listen <addr>       USB/IP 监听地址（默认 0.0.0.0:3240）
  --target <host:port>  ESP32 DAP TCP 地址（默认 192.168.137.96:8080）
  --web <addr>          Web 控制台监听地址（默认 127.0.0.1:3241）
  -h, --help            显示本帮助

环境变量:
  RUST_LOG              日志级别，例如 RUST_LOG=debug（默认 info）
";

impl Args {
    fn parse<I: Iterator<Item = String>>(mut it: I) -> Result<Self, String> {
        let mut listen = "0.0.0.0:3240".to_string();
        let mut target = "192.168.137.96:8080".to_string();
        let mut web = "127.0.0.1:3241".to_string();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--listen" => listen = it.next().ok_or("--listen 缺少参数值")?,
                "--target" => target = it.next().ok_or("--target 缺少参数值")?,
                "--web" => web = it.next().ok_or("--web 缺少参数值")?,
                "-h" | "--help" => return Err(USAGE.to_string()),
                other => return Err(format!("未知参数: {other}\n\n{USAGE}")),
            }
        }
        Ok(Self { listen, target, web })
    }
}

#[tokio::main]
async fn main() {
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            return;
        }
    };

    // 日志中枢要在 tracing 初始化之前就绪
    logs::init();
    metrics::mark_start();

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(logs::HubLayer)
        .init();

    tracing::info!(
        "✨ USB/IP Server 启动: 监听 {} → 目标 DAP {}",
        args.listen,
        args.target
    );

    {
        let web = args.web.clone();
        let target = args.target.clone();
        let listen = args.listen.clone();
        tokio::spawn(async move {
            if let Err(e) = web::serve(&web, target, listen).await {
                tracing::error!("Web 控制台异常退出: {e}");
            }
        });
    }

    if let Err(e) = usbip::serve(&args.listen, &args.target).await {
        tracing::error!("服务器异常退出: {e}");
    }
}
