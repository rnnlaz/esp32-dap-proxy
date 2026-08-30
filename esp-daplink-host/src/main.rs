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
        let mut target = "192.168.137.151:8080".to_string();
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
