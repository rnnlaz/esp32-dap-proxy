//! 桥接层：把 USB/IP 的 bulk EP1 语义映射到 ESP32 链路。
//!
//! CMSIS-DAP v2 的时序是严格一问一答：
//!
//! ```text
//! 宿主 OUT（DAP 命令） → 探头 IN（DAP 响应）
//! ```
//!
//! 因此每个 OUT 转发到链路后把响应缓存，随后到来的 IN 直接取走；
//! 多余的 IN（无数据可给）返回 0 长度，宿主 USB 栈会把它当作短读/超时处理。

use std::collections::VecDeque;

use crate::link::{DapLink, LinkError};

/// USB/IP 会话 ↔ ESP32 之间的桥。每个 USB/IP IMPORT 会话独享一个实例，
/// 与 ESP32 侧「单 TCP 客户端」的服务模型一一对应。
pub struct DapBridge {
    link: DapLink,
    /// 已就绪、待宿主 IN 读取的 DAP 响应队列
    pending: VecDeque<Vec<u8>>,
}

impl DapBridge {
    pub fn new(target: String) -> Self {
        Self {
            link: DapLink::new(target),
            pending: VecDeque::new(),
        }
    }

    /// EP1 OUT：转发 DAP 命令并等待响应入队。
    pub async fn bulk_out(&mut self, command: &[u8]) -> Result<(), LinkError> {
        // 若存在上一周期遗留、宿主始终未取走的响应（典型成因：宿主侧 USB
        // 超时触发 URB unlink、整周期作废重试），必须在此丢弃，否则响应流
        // 与命令流永久错位一格——后续每条命令都会拿到上一条命令的响应。
        if let Some(stale) = self.pending.pop_front() {
            tracing::debug!(
                "丢弃未取走的陈旧响应（{} 字节），重新对齐命令/响应流",
                stale.len()
            );
        }
        let response = self.link.request(command).await?;
        if !response.is_empty() {
            self.pending.push_back(response);
        }
        Ok(())
    }

    /// EP1 IN：取走一条缓存的响应，最多 `max_len` 字节（余量保留到下次读取）。
    pub async fn bulk_in(&mut self, max_len: usize) -> Result<Vec<u8>, LinkError> {
        let Some(response) = self.pending.pop_front() else {
            return Ok(Vec::new());
        };
        if response.len() <= max_len {
            return Ok(response);
        }
        // 防御性切分：正常情况下响应不会超过 USB 报文长度
        let (head, rest) = response.split_at(max_len);
        self.pending.push_front(rest.to_vec());
        Ok(head.to_vec())
    }
}
