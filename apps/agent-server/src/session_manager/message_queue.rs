use serde::{Deserialize, Serialize};

/// 排队的消息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueuedMessage {
    /// 唯一标识，用于删除
    pub id: String,
    /// 消息内容
    pub content: String,
    /// 入队时间戳
    pub queued_at_ms: u64,
}

/// QueueMessage 返回结果
#[derive(Serialize)]
pub struct QueueMessageResponse {
    pub status: QueueMessageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueMessageStatus {
    Started,
    Queued,
}

/// 生成消息 ID
pub(crate) fn generate_message_id() -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // 使用简单的计数器替代 rand
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let random = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("msg_{:016x}{:08x}", timestamp, random)
}

/// 消息队列最大长度
pub const MAX_QUEUE_SIZE: usize = 10;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_message_id() {
        let id1 = generate_message_id();
        let id2 = generate_message_id();
        assert!(id1.starts_with("msg_"));
        assert_ne!(id1, id2);
    }
}
