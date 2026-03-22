use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::config::WeixinClientConfig;
use crate::error::WeixinClientError;
use crate::media::WeixinMediaClient;
use crate::protocol::{GetUpdatesResponse, InboundMessage, MessageItemType, WeixinApiEnvelope};

const DEFAULT_BOT_TYPE: &str = "3";
const MESSAGE_TYPE_USER: i64 = 1;
const MESSAGE_TYPE_BOT: i64 = 2;
#[derive(Debug, Clone)]
pub struct WeixinClient {
    http: reqwest::Client,
    config: WeixinClientConfig,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SendTextRequest {
    pub to_user_id: String,
    pub context_token: String,
    pub text: String,
}

impl SendTextRequest {
    pub fn new(
        to_user_id: impl Into<String>,
        context_token: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            to_user_id: to_user_id.into(),
            context_token: context_token.into(),
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypingStatusRequest {
    pub ilink_user_id: String,
    pub typing_ticket: String,
    pub status: i64,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct LoginQr {
    #[serde(default)]
    pub qrcode: String,
    #[serde(default, rename = "qrcode_img_content")]
    pub qrcode_url: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LoginStatus {
    pub status: String,
    pub bot_token: String,
    pub account_id: String,
    pub base_url: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SendMessageResponse {
    pub message_id: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
struct TypingTicketResponse {
    #[serde(default)]
    typing_ticket: String,
    #[serde(flatten)]
    envelope: WeixinApiEnvelope,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
struct LoginStatusResponse {
    #[serde(default)]
    status: String,
    #[serde(default, rename = "bot_token")]
    bot_token: String,
    #[serde(default, rename = "ilink_bot_id")]
    account_id: String,
    #[serde(default, rename = "baseurl")]
    base_url: String,
    #[serde(default, rename = "ilink_user_id")]
    user_id: String,
}

pub trait InboundMessageExt {
    fn extract_inbound_text(&self) -> String;
    fn has_unsupported_inbound_media(&self) -> bool;
    fn should_handle_inbound_message(&self) -> bool;
}

impl InboundMessageExt for InboundMessage {
    fn extract_inbound_text(&self) -> String {
        self.item_list
            .iter()
            .filter_map(|item| match item.type_id {
                x if x == MessageItemType::Text as i64 => {
                    item.text_item.as_ref().map(|value| value.text.trim().to_owned())
                }
                x if x == MessageItemType::Voice as i64 => {
                    item.voice_item.as_ref().map(|value| value.text.trim().to_owned())
                }
                _ => None,
            })
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn has_unsupported_inbound_media(&self) -> bool {
        self.item_list.iter().any(|item| match item.type_id {
            x if x == MessageItemType::Text as i64 => false,
            x if x == MessageItemType::Voice as i64 => {
                item.voice_item.as_ref().map(|value| value.text.trim().is_empty()).unwrap_or(true)
            }
            x if x == MessageItemType::Image as i64
                || x == MessageItemType::Voice as i64
                || x == MessageItemType::File as i64
                || x == MessageItemType::Video as i64 =>
            {
                true
            }
            _ => false,
        })
    }

    fn should_handle_inbound_message(&self) -> bool {
        self.message_type == MESSAGE_TYPE_USER && !self.from_user_id.trim().is_empty()
    }
}

impl WeixinClient {
    pub fn new(config: WeixinClientConfig) -> Result<Self, WeixinClientError> {
        let http = reqwest::Client::builder().build().map_err(|error| {
            WeixinClientError::new(format!("创建微信 HTTP 客户端失败: {error}"))
        })?;
        Ok(Self { http, config })
    }

    pub fn media(&self) -> WeixinMediaClient {
        WeixinMediaClient::new(self.clone())
    }

    pub fn split_text_for_weixin(text: &str, max_length: usize) -> Vec<String> {
        let raw = text.replace('\r', "");
        if raw.is_empty() {
            return Vec::new();
        }

        let max = max_length.max(200);
        let chars: Vec<char> = raw.chars().collect();
        let mut chunks = Vec::new();
        let mut cursor = 0_usize;

        while cursor < chars.len() {
            let mut end = (cursor + max).min(chars.len());
            if end < chars.len() {
                let lower_bound = cursor + ((max as f32 * 0.6) as usize);
                if let Some(newline_index) = chars[cursor..end].iter().rposition(|ch| *ch == '\n') {
                    let absolute = cursor + newline_index;
                    if absolute > lower_bound {
                        end = absolute + 1;
                    }
                }
            }
            if end <= cursor {
                end = (cursor + max).min(chars.len());
            }
            chunks.push(chars[cursor..end].iter().collect::<String>());
            cursor = end;
        }

        chunks
    }

    pub async fn fetch_login_qr(
        &self,
        bot_type: Option<&str>,
    ) -> Result<LoginQr, WeixinClientError> {
        let bot_type = bot_type.unwrap_or(DEFAULT_BOT_TYPE).trim();
        let url = format!(
            "{}ilink/bot/get_bot_qrcode?bot_type={}",
            self.config.base_url,
            url_encode(bot_type)
        );
        self.get_json::<LoginQr>(&url, self.config.api_timeout, false).await
    }

    pub async fn poll_login_status(&self, qrcode: &str) -> Result<LoginStatus, WeixinClientError> {
        let url = format!(
            "{}ilink/bot/get_qrcode_status?qrcode={}",
            self.config.base_url,
            url_encode(qrcode.trim())
        );
        let headers = self.route_headers(true)?;

        match self
            .get_json_with_headers::<LoginStatusResponse>(
                &url,
                headers,
                self.config.qr_poll_timeout,
                true,
            )
            .await
        {
            Ok(response) => Ok(LoginStatus {
                status: normalize_string(&response.status).unwrap_or_else(|| "wait".to_owned()),
                bot_token: response.bot_token.trim().to_owned(),
                account_id: response.account_id.trim().to_owned(),
                base_url: response.base_url.trim().to_owned(),
                user_id: response.user_id.trim().to_owned(),
            }),
            Err(error) if error.to_string().contains("请求超时") => Ok(LoginStatus {
                status: "wait".into(),
                bot_token: String::new(),
                account_id: String::new(),
                base_url: String::new(),
                user_id: String::new(),
            }),
            Err(error) => Err(error),
        }
    }

    pub async fn get_updates(
        &self,
        cursor: Option<&str>,
    ) -> Result<GetUpdatesResponse, WeixinClientError> {
        let body = json!({
            "get_updates_buf": cursor.unwrap_or("").trim(),
            "base_info": {},
        });

        match self
            .post_json::<GetUpdatesResponse>(
                "ilink/bot/getupdates",
                &body,
                self.config.long_poll_timeout,
                true,
            )
            .await
        {
            Ok(response) => Ok(response),
            Err(error) if error.to_string().contains("请求超时") => Ok(GetUpdatesResponse {
                ret: 0,
                msgs: Vec::new(),
                get_updates_buf: cursor.unwrap_or("").trim().to_owned(),
            }),
            Err(error) => Err(error),
        }
    }

    pub async fn send_text(
        &self,
        request: SendTextRequest,
    ) -> Result<SendMessageResponse, WeixinClientError> {
        let to_user_id = ensure_non_empty(request.to_user_id, "缺少 to_user_id，无法发送微信回复")?;
        let context_token =
            ensure_non_empty(request.context_token, "缺少 context_token，无法发送微信回复")?;
        let client_id = generate_client_id()?;
        let item_list = if request.text.is_empty() {
            Vec::<Value>::new()
        } else {
            vec![json!({
                "type": MessageItemType::Text as i64,
                "text_item": { "text": request.text },
            })]
        };

        let body = json!({
            "msg": {
                "from_user_id": "",
                "to_user_id": to_user_id,
                "client_id": client_id,
                "message_type": MESSAGE_TYPE_BOT,
                "message_state": 2,
                "item_list": item_list,
                "context_token": context_token,
            },
            "base_info": {},
        });

        let payload = self
            .post_json::<Value>("ilink/bot/sendmessage", &body, self.config.api_timeout, true)
            .await?;
        assert_weixin_ok_response(&payload, "sendmessage")?;
        Ok(SendMessageResponse { message_id: client_id })
    }

    pub async fn get_typing_ticket(
        &self,
        ilink_user_id: &str,
        context_token: Option<&str>,
    ) -> Result<String, WeixinClientError> {
        let body = json!({
            "ilink_user_id": ensure_non_empty(ilink_user_id.to_owned(), "缺少 ilink_user_id")?,
            "context_token": normalize_string(context_token.unwrap_or("")),
            "base_info": {},
        });
        let response = self
            .post_json::<TypingTicketResponse>(
                "ilink/bot/getconfig",
                &body,
                self.config.config_timeout,
                true,
            )
            .await?;
        assert_weixin_ok_response(
            &json!({
                "errcode": response.envelope.errcode,
                "ret": response.envelope.ret,
                "errmsg": response.envelope.errmsg,
            }),
            "getconfig",
        )?;
        Ok(response.typing_ticket.trim().to_owned())
    }

    pub async fn send_typing_status(
        &self,
        request: TypingStatusRequest,
    ) -> Result<(), WeixinClientError> {
        if request.ilink_user_id.trim().is_empty() || request.typing_ticket.trim().is_empty() {
            return Ok(());
        }

        let body = json!({
            "ilink_user_id": request.ilink_user_id,
            "typing_ticket": request.typing_ticket,
            "status": request.status,
            "base_info": {},
        });
        let response = self
            .post_json::<Value>("ilink/bot/sendtyping", &body, self.config.config_timeout, true)
            .await?;
        assert_weixin_ok_response(&response, "sendtyping")?;
        Ok(())
    }

    pub(crate) fn config(&self) -> &WeixinClientConfig {
        &self.config
    }

    pub(crate) async fn post_json<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &Value,
        timeout: std::time::Duration,
        authenticated: bool,
    ) -> Result<T, WeixinClientError> {
        let url = format!("{}{}", self.config.base_url, endpoint);
        let headers = self.auth_headers(authenticated)?;
        let response = self
            .http
            .post(url)
            .headers(headers)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .timeout(timeout)
            .body(body.to_string())
            .send()
            .await
            .map_err(map_reqwest_error)?;

        decode_json_response(response).await
    }

    pub(crate) async fn post_bytes(
        &self,
        url: &str,
        body: Vec<u8>,
        content_type: &'static str,
    ) -> Result<reqwest::Response, WeixinClientError> {
        self.http
            .post(url)
            .header(CONTENT_TYPE, HeaderValue::from_static(content_type))
            .body(body)
            .send()
            .await
            .map_err(map_reqwest_error)
    }

    pub(crate) async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, WeixinClientError> {
        let response = self.http.get(url).send().await.map_err(map_reqwest_error)?;
        let status = response.status();
        let body = response.bytes().await.map_err(map_reqwest_error)?.to_vec();
        if !status.is_success() {
            return Err(WeixinClientError::new(format!("GET {url} {}", status.as_u16()))
                .with_status_code(Some(status.as_u16()))
                .with_response_body(Some(String::from_utf8_lossy(&body).into_owned())));
        }
        Ok(body)
    }

    fn auth_headers(&self, authenticated: bool) -> Result<HeaderMap, WeixinClientError> {
        let mut headers = self.route_headers(false)?;
        if authenticated {
            headers.insert(
                HeaderName::from_static("authorizationtype"),
                HeaderValue::from_static("ilink_bot_token"),
            );
            headers.insert(
                HeaderName::from_static("x-wechat-uin"),
                HeaderValue::from_str(&random_wechat_uin()?).map_err(|error| {
                    WeixinClientError::new(format!("构造 X-WECHAT-UIN 失败: {error}"))
                })?,
            );
            let token =
                self.config
                    .bot_token
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| WeixinClientError::new("缺少 bot token，无法调用微信接口"))?;
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}")).map_err(|error| {
                    WeixinClientError::new(format!("构造 Authorization 头失败: {error}"))
                })?,
            );
        }
        Ok(headers)
    }

    fn route_headers(&self, include_client_version: bool) -> Result<HeaderMap, WeixinClientError> {
        let mut headers = HeaderMap::new();
        if let Some(route_tag) =
            self.config.route_tag.as_deref().filter(|value| !value.trim().is_empty())
        {
            headers.insert(
                HeaderName::from_static("skroutetag"),
                HeaderValue::from_str(route_tag).map_err(|error| {
                    WeixinClientError::new(format!("构造 SKRouteTag 头失败: {error}"))
                })?,
            );
        }
        if include_client_version {
            headers.insert(
                HeaderName::from_static("ilink-app-clientversion"),
                HeaderValue::from_static("1"),
            );
        }
        Ok(headers)
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        timeout: std::time::Duration,
        include_client_version: bool,
    ) -> Result<T, WeixinClientError> {
        let headers = self.route_headers(include_client_version)?;
        self.get_json_with_headers(url, headers, timeout, include_client_version).await
    }

    async fn get_json_with_headers<T: DeserializeOwned>(
        &self,
        url: &str,
        headers: HeaderMap,
        timeout: std::time::Duration,
        _include_client_version: bool,
    ) -> Result<T, WeixinClientError> {
        let response = self
            .http
            .get(url)
            .headers(headers)
            .timeout(timeout)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        decode_json_response(response).await
    }
}

pub(crate) fn assert_weixin_ok_response(
    payload: &Value,
    label: &str,
) -> Result<(), WeixinClientError> {
    let code = payload
        .get("errcode")
        .and_then(Value::as_i64)
        .or_else(|| payload.get("ret").and_then(Value::as_i64))
        .unwrap_or(0);
    if code == 0 {
        return Ok(());
    }
    let message = payload
        .get("errmsg")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("errcode={code}"));
    Err(WeixinClientError::new(format!("{label} failed: {message}")))
}

fn ensure_non_empty(value: String, message: &str) -> Result<String, WeixinClientError> {
    let trimmed = value.trim();
    if trimmed.is_empty() { Err(WeixinClientError::new(message)) } else { Ok(trimmed.to_owned()) }
}

fn normalize_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_owned()) }
}

fn generate_client_id() -> Result<String, WeixinClientError> {
    Ok(format!("rust-weixin-{}-{}", now_millis()?, hex_random(4)?))
}

fn random_wechat_uin() -> Result<String, WeixinClientError> {
    use base64::Engine;
    let bytes = random_bytes::<4>()?;
    let value = u32::from_be_bytes(bytes);
    Ok(base64::engine::general_purpose::STANDARD.encode(value.to_string().as_bytes()))
}

pub(crate) fn now_millis() -> Result<u128, WeixinClientError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| WeixinClientError::new(format!("读取系统时间失败: {error}")))
}

pub(crate) fn hex_random(length: usize) -> Result<String, WeixinClientError> {
    let mut bytes = vec![0_u8; length];
    fill_random(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(crate) fn fill_random(buffer: &mut [u8]) -> Result<(), WeixinClientError> {
    getrandom::fill(buffer)
        .map_err(|error| WeixinClientError::new(format!("生成随机字节失败: {error}")))
}

pub(crate) fn random_bytes<const N: usize>() -> Result<[u8; N], WeixinClientError> {
    let mut bytes = [0_u8; N];
    fill_random(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn url_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn map_reqwest_error(error: reqwest::Error) -> WeixinClientError {
    if error.is_timeout() {
        WeixinClientError::new("请求超时")
    } else {
        WeixinClientError::new(format!("微信接口请求失败: {error}"))
    }
}

async fn decode_json_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, WeixinClientError> {
    let status = response.status();
    let text = response.text().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(WeixinClientError::new(format!("请求失败: {}", status.as_u16()))
            .with_status_code(Some(status.as_u16()))
            .with_response_body(Some(text)));
    }
    if text.trim().is_empty() {
        return serde_json::from_value(Value::Object(Default::default()))
            .map_err(|error| WeixinClientError::new(format!("解析空响应失败: {error}")));
    }
    serde_json::from_str(&text).map_err(|error| {
        WeixinClientError::new(format!("解析微信响应失败: {error}")).with_response_body(Some(text))
    })
}
