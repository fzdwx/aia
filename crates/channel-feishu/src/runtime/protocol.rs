use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

pub(super) const FEISHU_HEADER_TYPE: &str = "type";
pub(super) const FEISHU_HEADER_MESSAGE_ID: &str = "message_id";
pub(super) const FEISHU_HEADER_SUM: &str = "sum";
pub(super) const FEISHU_HEADER_SEQ: &str = "seq";
pub(super) const FEISHU_HEADER_BIZ_RT: &str = "biz_rt";
pub(super) const FEISHU_HEADER_SERVICE_ID: &str = "service_id";
pub(super) const FEISHU_MESSAGE_TYPE_PING: &str = "ping";
pub(super) const FEISHU_MESSAGE_TYPE_PONG: &str = "pong";
pub(super) const FEISHU_FRAME_TYPE_CONTROL: i32 = 0;
pub(super) const FEISHU_FRAME_TYPE_DATA: i32 = 1;

#[derive(Debug, Deserialize)]
pub(super) struct EventEnvelope {
    pub(super) header: EventHeader,
    pub(super) event: EventBody,
}

#[derive(Debug, Deserialize)]
pub(super) struct EventHeader {
    pub(super) event_type: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct EventBody {
    pub(super) sender: Sender,
    pub(super) message: Message,
    #[serde(default)]
    pub(super) chat: Option<Chat>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Sender {
    pub(super) sender_id: SenderId,
    pub(super) sender_type: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct SenderId {
    pub(super) open_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct Message {
    pub(super) message_id: String,
    pub(super) message_type: String,
    pub(super) content: String,
    pub(super) chat_type: String,
    #[serde(default)]
    pub(super) chat_id: Option<String>,
    #[serde(default)]
    pub(super) thread_id: Option<String>,
    #[serde(default)]
    pub(super) mentions: Vec<Mention>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Chat {
    pub(super) chat_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct Mention {
    pub(super) key: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct TextContent {
    pub(super) text: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct TenantAccessTokenResponse {
    pub(super) code: i32,
    pub(super) msg: String,
    pub(super) tenant_access_token: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct FeishuSimpleResponse {
    pub(super) code: i32,
    pub(super) msg: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct FeishuMessageResponse {
    pub(super) code: i32,
    pub(super) msg: String,
    pub(super) data: Option<FeishuMessageResponseData>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FeishuMessageResponseData {
    pub(super) message_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FeishuCardKitCreateResponse {
    pub(super) code: i32,
    pub(super) msg: String,
    pub(super) data: Option<FeishuCardKitCreateResponseData>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FeishuCardKitCreateResponseData {
    pub(super) card_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FeishuReactionCreateResponse {
    pub(super) code: i32,
    pub(super) msg: String,
    pub(super) data: Option<FeishuReactionCreateData>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FeishuReactionCreateData {
    pub(super) reaction_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FeishuP2pChatQueryResponse {
    pub(super) code: i32,
    pub(super) msg: String,
    pub(super) data: Option<FeishuP2pChatQueryData>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FeishuP2pChatQueryData {
    pub(super) p2p_chats: Option<Vec<FeishuP2pChatItem>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FeishuP2pChatItem {
    pub(super) chat_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct FeishuWebsocketEndpointResponse {
    pub(super) code: i32,
    pub(super) msg: String,
    pub(super) data: Option<FeishuWebsocketEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct FeishuWebsocketEndpoint {
    #[serde(rename = "URL")]
    pub(super) url: String,
    #[serde(rename = "ClientConfig")]
    pub(super) client_config: Option<FeishuServerClientConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct FeishuServerClientConfig {
    #[serde(rename = "ReconnectCount", default)]
    pub(super) reconnect_count: i64,
    #[serde(rename = "ReconnectInterval", default)]
    pub(super) reconnect_interval: u64,
    #[serde(rename = "ReconnectNonce", default)]
    pub(super) reconnect_nonce: u64,
    #[serde(rename = "PingInterval", default)]
    pub(super) ping_interval: u64,
}

#[derive(Debug, Clone)]
pub(super) struct FeishuConnectionPolicy {
    pub(super) reconnect_interval: Duration,
    pub(super) ping_interval: Duration,
    reconnect_count: Option<usize>,
    reconnect_nonce: Duration,
}

impl Default for FeishuConnectionPolicy {
    fn default() -> Self {
        Self {
            reconnect_interval: Duration::from_secs(120),
            ping_interval: Duration::from_secs(120),
            reconnect_count: None,
            reconnect_nonce: Duration::from_secs(30),
        }
    }
}

impl FeishuConnectionPolicy {
    pub(super) fn apply_server_config(&mut self, config: Option<&FeishuServerClientConfig>) {
        let Some(config) = config else {
            return;
        };
        if config.reconnect_interval > 0 {
            self.reconnect_interval = Duration::from_secs(config.reconnect_interval);
        }
        if config.ping_interval > 0 {
            self.ping_interval = Duration::from_secs(config.ping_interval);
        }
        self.reconnect_count =
            (config.reconnect_count >= 0).then_some(config.reconnect_count as usize);
        if config.reconnect_nonce > 0 {
            self.reconnect_nonce = Duration::from_secs(config.reconnect_nonce);
        }
    }

    pub(super) fn should_stop_reconnecting(&self, attempts: usize) -> bool {
        self.reconnect_count.is_some_and(|max| attempts >= max)
    }

    pub(super) fn next_reconnect_jitter(&self) -> Option<Duration> {
        let upper_bound_ms = self.reconnect_nonce.as_millis();
        if upper_bound_ms == 0 {
            return None;
        }
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        Some(Duration::from_millis((nanos % upper_bound_ms) as u64))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FeishuHeader {
    key: String,
    value: String,
}

impl FeishuHeader {
    pub(super) fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self { key: key.into(), value: value.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FeishuFrame {
    pub(super) seq_id: u64,
    pub(super) log_id: u64,
    pub(super) service: i32,
    pub(super) method: i32,
    pub(super) headers: Vec<FeishuHeader>,
    pub(super) payload_encoding: String,
    pub(super) payload_type: String,
    pub(super) payload: Vec<u8>,
    pub(super) log_id_new: String,
}

impl FeishuFrame {
    pub(super) fn new_ping(service_id: i32) -> Self {
        Self {
            seq_id: 0,
            log_id: 0,
            service: service_id,
            method: FEISHU_FRAME_TYPE_CONTROL,
            headers: vec![FeishuHeader::new(FEISHU_HEADER_TYPE, FEISHU_MESSAGE_TYPE_PING)],
            payload_encoding: String::new(),
            payload_type: String::new(),
            payload: Vec::new(),
            log_id_new: String::new(),
        }
    }

    pub(super) fn with_response_payload(
        &self,
        headers: Vec<FeishuHeader>,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            seq_id: self.seq_id,
            log_id: self.log_id,
            service: self.service,
            method: self.method,
            headers,
            payload_encoding: self.payload_encoding.clone(),
            payload_type: self.payload_type.clone(),
            payload,
            log_id_new: self.log_id_new.clone(),
        }
    }

    pub(super) fn encode(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        encode_varint_field(&mut buffer, 1, self.seq_id);
        encode_varint_field(&mut buffer, 2, self.log_id);
        encode_varint_field(&mut buffer, 3, self.service as u64);
        encode_varint_field(&mut buffer, 4, self.method as u64);
        for header in &self.headers {
            let mut header_buffer = Vec::new();
            encode_length_delimited_field(&mut header_buffer, 1, header.key.as_bytes());
            encode_length_delimited_field(&mut header_buffer, 2, header.value.as_bytes());
            encode_length_delimited_field(&mut buffer, 5, &header_buffer);
        }
        encode_length_delimited_field(&mut buffer, 6, self.payload_encoding.as_bytes());
        encode_length_delimited_field(&mut buffer, 7, self.payload_type.as_bytes());
        if !self.payload.is_empty() {
            encode_length_delimited_field(&mut buffer, 8, &self.payload);
        }
        encode_length_delimited_field(&mut buffer, 9, self.log_id_new.as_bytes());
        buffer
    }

    pub(super) fn decode(input: &[u8]) -> Result<Self, String> {
        let mut cursor = 0;
        let mut frame = Self {
            seq_id: 0,
            log_id: 0,
            service: 0,
            method: 0,
            headers: Vec::new(),
            payload_encoding: String::new(),
            payload_type: String::new(),
            payload: Vec::new(),
            log_id_new: String::new(),
        };

        while cursor < input.len() {
            let tag = decode_varint(input, &mut cursor)?;
            let field_number = tag >> 3;
            let wire_type = tag & 0x07;
            match (field_number, wire_type) {
                (1, 0) => frame.seq_id = decode_varint(input, &mut cursor)?,
                (2, 0) => frame.log_id = decode_varint(input, &mut cursor)?,
                (3, 0) => frame.service = decode_varint(input, &mut cursor)? as i32,
                (4, 0) => frame.method = decode_varint(input, &mut cursor)? as i32,
                (5, 2) => {
                    let bytes = decode_length_delimited(input, &mut cursor)?;
                    frame.headers.push(decode_header(bytes)?);
                }
                (6, 2) => {
                    frame.payload_encoding =
                        String::from_utf8(decode_length_delimited(input, &mut cursor)?.to_vec())
                            .map_err(|error| error.to_string())?;
                }
                (7, 2) => {
                    frame.payload_type =
                        String::from_utf8(decode_length_delimited(input, &mut cursor)?.to_vec())
                            .map_err(|error| error.to_string())?;
                }
                (8, 2) => frame.payload = decode_length_delimited(input, &mut cursor)?.to_vec(),
                (9, 2) => {
                    frame.log_id_new =
                        String::from_utf8(decode_length_delimited(input, &mut cursor)?.to_vec())
                            .map_err(|error| error.to_string())?;
                }
                (_, _) => skip_unknown_field(input, &mut cursor, wire_type)?,
            }
        }

        Ok(frame)
    }
}

pub(super) struct FeishuHeaders<'a> {
    inner: &'a [FeishuHeader],
}

impl<'a> FeishuHeaders<'a> {
    pub(super) fn new(inner: &'a [FeishuHeader]) -> Self {
        Self { inner }
    }

    pub(super) fn get_string(&self, key: &str) -> Option<&str> {
        self.inner.iter().find_map(|header| (header.key == key).then_some(header.value.as_str()))
    }

    pub(super) fn get_usize(&self, key: &str) -> Option<usize> {
        self.get_string(key)?.parse().ok()
    }
}

#[derive(Default)]
pub(super) struct PendingFrameBuffer {
    pending: HashMap<String, PendingMessageFrame>,
}

impl PendingFrameBuffer {
    pub(super) fn push(
        &mut self,
        message_id: String,
        sum: usize,
        seq: usize,
        payload: Vec<u8>,
    ) -> Option<Vec<u8>> {
        let entry =
            self.pending.entry(message_id.clone()).or_insert_with(|| PendingMessageFrame::new(sum));
        if seq >= entry.parts.len() {
            return None;
        }
        entry.parts[seq] = Some(payload);
        if !entry.parts.iter().all(Option::is_some) {
            return None;
        }
        let completed = self.pending.remove(&message_id)?;
        Some(completed.parts.into_iter().flatten().flatten().collect())
    }
}

struct PendingMessageFrame {
    parts: Vec<Option<Vec<u8>>>,
}

impl PendingMessageFrame {
    fn new(sum: usize) -> Self {
        Self { parts: vec![None; sum] }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct FeishuWsResponse {
    code: i32,
    headers: Option<HashMap<String, String>>,
    data: Option<Vec<u8>>,
}

impl FeishuWsResponse {
    pub(super) fn ok() -> Self {
        Self { code: 200, headers: None, data: None }
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|error| error.to_string())
    }
}

fn encode_varint_field(buffer: &mut Vec<u8>, field_number: u64, value: u64) {
    encode_varint(buffer, field_number << 3);
    encode_varint(buffer, value);
}

fn encode_length_delimited_field(buffer: &mut Vec<u8>, field_number: u64, value: &[u8]) {
    encode_varint(buffer, (field_number << 3) | 0x02);
    encode_varint(buffer, value.len() as u64);
    buffer.extend_from_slice(value);
}

fn encode_varint(buffer: &mut Vec<u8>, mut value: u64) {
    loop {
        if value < 0x80 {
            buffer.push(value as u8);
            return;
        }
        buffer.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
}

fn decode_varint(input: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let mut shift = 0;
    let mut value = 0_u64;
    loop {
        if *cursor >= input.len() {
            return Err("飞书帧解码遇到意外结尾".into());
        }
        let byte = input[*cursor];
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Ok(value);
        }
        shift += 7;
        if shift >= 64 {
            return Err("飞书帧 varint 溢出".into());
        }
    }
}

fn decode_length_delimited<'a>(input: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], String> {
    let len = decode_varint(input, cursor)? as usize;
    if *cursor + len > input.len() {
        return Err("飞书帧长度字段越界".into());
    }
    let bytes = &input[*cursor..*cursor + len];
    *cursor += len;
    Ok(bytes)
}

fn decode_header(input: &[u8]) -> Result<FeishuHeader, String> {
    let mut cursor = 0;
    let mut key = None;
    let mut value = None;
    while cursor < input.len() {
        let tag = decode_varint(input, &mut cursor)?;
        let field_number = tag >> 3;
        let wire_type = tag & 0x07;
        match (field_number, wire_type) {
            (1, 2) => {
                key = Some(
                    String::from_utf8(decode_length_delimited(input, &mut cursor)?.to_vec())
                        .map_err(|error| error.to_string())?,
                );
            }
            (2, 2) => {
                value = Some(
                    String::from_utf8(decode_length_delimited(input, &mut cursor)?.to_vec())
                        .map_err(|error| error.to_string())?,
                );
            }
            (_, _) => skip_unknown_field(input, &mut cursor, wire_type)?,
        }
    }
    Ok(FeishuHeader {
        key: key.ok_or_else(|| "飞书帧 header 缺少 key".to_string())?,
        value: value.ok_or_else(|| "飞书帧 header 缺少 value".to_string())?,
    })
}

fn skip_unknown_field(input: &[u8], cursor: &mut usize, wire_type: u64) -> Result<(), String> {
    match wire_type {
        0 => {
            let _ = decode_varint(input, cursor)?;
        }
        1 => {
            *cursor = cursor.checked_add(8).ok_or_else(|| "飞书帧游标溢出".to_string())?;
        }
        2 => {
            let len = decode_varint(input, cursor)? as usize;
            *cursor = cursor.checked_add(len).ok_or_else(|| "飞书帧游标溢出".to_string())?;
        }
        5 => {
            *cursor = cursor.checked_add(4).ok_or_else(|| "飞书帧游标溢出".to_string())?;
        }
        _ => return Err(format!("飞书帧不支持的 wire type: {wire_type}")),
    }
    if *cursor > input.len() {
        return Err("飞书帧跳过未知字段越界".into());
    }
    Ok(())
}
