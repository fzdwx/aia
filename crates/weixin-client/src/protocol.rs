use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct WeixinApiEnvelope {
    #[serde(default)]
    pub errcode: Option<i64>,
    #[serde(default)]
    pub ret: Option<i64>,
    #[serde(default)]
    pub errmsg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct GetUpdatesResponse {
    #[serde(default)]
    pub ret: i64,
    #[serde(default)]
    pub msgs: Vec<InboundMessage>,
    #[serde(default)]
    pub get_updates_buf: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct InboundMessage {
    #[serde(default)]
    pub message_id: Option<i64>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub seq: Option<i64>,
    #[serde(default)]
    pub create_time_ms: Option<i64>,
    #[serde(default)]
    pub from_user_id: String,
    #[serde(default)]
    pub to_user_id: String,
    #[serde(default)]
    pub context_token: String,
    #[serde(default)]
    pub message_type: i64,
    #[serde(default)]
    pub item_list: Vec<MessageItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct MessageItem {
    #[serde(rename = "type")]
    pub type_id: i64,
    #[serde(default)]
    pub text_item: Option<TextItem>,
    #[serde(default)]
    pub voice_item: Option<VoiceItem>,
    #[serde(default)]
    pub image_item: Option<ImageItem>,
    #[serde(default)]
    pub file_item: Option<FileItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct TextItem {
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct VoiceItem {
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct ImageItem {
    #[serde(default)]
    pub media: MediaDescriptor,
    #[serde(default)]
    pub aeskey: String,
    #[serde(default)]
    pub mid_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct FileItem {
    #[serde(default)]
    pub media: MediaDescriptor,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub len: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct MediaDescriptor {
    #[serde(default)]
    pub encrypt_query_param: String,
    #[serde(default)]
    pub aes_key: String,
    #[serde(default)]
    pub encrypt_type: i64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MessageItemType {
    Text = 1,
    Image = 2,
    Voice = 3,
    File = 4,
    Video = 5,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MediaType {
    Image = 1,
    File = 3,
}
