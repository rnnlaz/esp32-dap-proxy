use crate::link::{DapLink, LinkError};

pub struct DapBridge {
    link: DapLink,
    /// 已就绪、待宿主 IN 读取的 DAP 响应（至多一条）
    pending: Option<Vec<u8>>,
}

impl DapBridge {
    pub fn new(target: String) -> Self {
        Self {
            link: DapLink::new(target),
            pending: None,
        }
    }

    /// EP1 OUT：转发 DAP 命令并等待响应入队。
    pub async fn bulk_out(&mut self, command: &[u8]) -> Result<(), LinkError> {
        // 丢弃上一周期可能遗留的响应（宿主超时放弃的周期），
        // 保持命令流与响应流一一对应。
        if let Some(stale) = self.pending.take() {
            tracing::debug!("丢弃未取走的陈旧响应（{} 字节）", stale.len());
        }
        let response = self.link.request(command).await?;
        if !response.is_empty() {
            self.pending = Some(response);
        }
        Ok(())
    }

    pub async fn bulk_in(&mut self, max_len: usize) -> Result<Vec<u8>, LinkError> {
        let Some(response) = self.pending.take() else {
            return Ok(Vec::new());
        };
        if response.len() <= max_len {
            return Ok(response);
        }
        let (head, rest) = response.split_at(max_len);
        self.pending = Some(rest.to_vec());
        Ok(head.to_vec())
    }
}
