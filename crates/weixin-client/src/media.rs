use std::path::{Path, PathBuf};

use aes::Aes128;
use base64::Engine;
use cipher::{BlockDecryptMut, BlockEncryptMut, KeyInit, block_padding::Pkcs7};
use ecb::{Decryptor, Encryptor};
use md5::{Digest, Md5};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{
    SendMessageResponse, WeixinClient, assert_weixin_ok_response, hex_random, now_millis,
    url_encode,
};
use crate::error::WeixinClientError;
use crate::protocol::{MediaType, MessageItemType};

#[derive(Debug, Clone)]
pub struct WeixinMediaClient {
    client: WeixinClient,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SendMediaFileRequest {
    pub to_user_id: String,
    pub context_token: String,
    pub file_path: PathBuf,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DownloadedAttachment {
    pub kind: String,
    pub path: PathBuf,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UploadUrlResponse {
    #[serde(default)]
    upload_param: String,
    #[serde(flatten)]
    envelope: crate::protocol::WeixinApiEnvelope,
}

struct UploadedMedia {
    download_encrypted_query_param: String,
    aes_key_hex: String,
    file_size: usize,
    file_size_ciphertext: usize,
}

impl SendMediaFileRequest {
    pub fn new(
        to_user_id: impl Into<String>,
        context_token: impl Into<String>,
        file_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            to_user_id: to_user_id.into(),
            context_token: context_token.into(),
            file_path: file_path.into(),
            text: None,
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

impl WeixinMediaClient {
    pub(crate) fn new(client: WeixinClient) -> Self {
        Self { client }
    }

    pub async fn send_media_file(
        &self,
        request: SendMediaFileRequest,
    ) -> Result<SendMessageResponse, WeixinClientError> {
        let resolved_path = request
            .file_path
            .canonicalize()
            .map_err(|error| WeixinClientError::new(format!("读取媒体文件路径失败: {error}")))?;
        let mime = mime_from_filename(&resolved_path);
        let media_type =
            if mime.starts_with("image/") { MediaType::Image } else { MediaType::File };

        let uploaded =
            self.upload_media_to_weixin(&resolved_path, &request.to_user_id, media_type).await?;

        let mut queue = Vec::new();
        if let Some(text) = request.text.as_deref().filter(|value| !value.is_empty()) {
            queue.push(json!({
                "type": MessageItemType::Text as i64,
                "text_item": { "text": text },
            }));
        }

        let media_item = if media_type == MediaType::Image {
            json!({
                "type": MessageItemType::Image as i64,
                "image_item": {
                    "media": {
                        "encrypt_query_param": uploaded.download_encrypted_query_param,
                        "aes_key": base64::engine::general_purpose::STANDARD.encode(hex_to_bytes(&uploaded.aes_key_hex)?),
                        "encrypt_type": 1,
                    },
                    "mid_size": uploaded.file_size_ciphertext,
                }
            })
        } else {
            json!({
                "type": MessageItemType::File as i64,
                "file_item": {
                    "media": {
                        "encrypt_query_param": uploaded.download_encrypted_query_param,
                        "aes_key": base64::engine::general_purpose::STANDARD.encode(hex_to_bytes(&uploaded.aes_key_hex)?),
                        "encrypt_type": 1,
                    },
                    "file_name": resolved_path.file_name().and_then(|name| name.to_str()).unwrap_or("attachment.bin"),
                    "len": uploaded.file_size.to_string(),
                }
            })
        };
        queue.push(media_item);

        let mut last_message_id = String::new();
        for item in queue {
            let client_id = format!("rust-weixin-{}-{}", now_millis()?, hex_random(4)?);
            let body = json!({
                "msg": {
                    "from_user_id": "",
                    "to_user_id": request.to_user_id,
                    "client_id": client_id,
                    "message_type": 2,
                    "message_state": 2,
                    "item_list": [item],
                    "context_token": request.context_token,
                },
                "base_info": {},
            });

            let response = self
                .client
                .post_json::<Value>(
                    "ilink/bot/sendmessage",
                    &body,
                    self.client.config().api_timeout,
                    true,
                )
                .await?;
            assert_weixin_ok_response(&response, "sendmessage")?;
            last_message_id = client_id;
        }

        Ok(SendMessageResponse { message_id: last_message_id })
    }

    pub async fn download_attachments(
        &self,
        message: &crate::protocol::InboundMessage,
        destination_dir: impl AsRef<Path>,
    ) -> Result<Vec<DownloadedAttachment>, WeixinClientError> {
        let destination_dir = destination_dir.as_ref();
        let mut attachments = Vec::new();

        for (index, item) in message.item_list.iter().enumerate() {
            if item.type_id == MessageItemType::Image as i64 {
                if let Some(image_item) = item.image_item.as_ref() {
                    let key = if !image_item.aeskey.trim().is_empty() {
                        hex_to_bytes(image_item.aeskey.trim())?
                    } else {
                        base64::engine::general_purpose::STANDARD
                            .decode(image_item.media.aes_key.trim())
                            .map_err(|error| {
                                WeixinClientError::new(format!("解析图片 AES key 失败: {error}"))
                            })?
                    };
                    let bytes = self
                        .download_and_decrypt_buffer(&image_item.media.encrypt_query_param, &key)
                        .await?;
                    let extension = guess_image_extension(&bytes);
                    let file_path =
                        destination_dir.join(format!("{}_{}{}", now_millis()?, index, extension));
                    tokio::fs::create_dir_all(destination_dir).await.map_err(|error| {
                        WeixinClientError::new(format!("创建附件目录失败: {error}"))
                    })?;
                    tokio::fs::write(&file_path, bytes).await.map_err(|error| {
                        WeixinClientError::new(format!("写入图片附件失败: {error}"))
                    })?;
                    attachments.push(DownloadedAttachment {
                        kind: "image".into(),
                        summary: format!("Image: {}", file_path.display()),
                        path: file_path,
                    });
                }
            }

            if item.type_id == MessageItemType::File as i64 {
                if let Some(file_item) = item.file_item.as_ref() {
                    let key = base64::engine::general_purpose::STANDARD
                        .decode(file_item.media.aes_key.trim())
                        .map_err(|error| {
                            WeixinClientError::new(format!("解析文件 AES key 失败: {error}"))
                        })?;
                    let bytes = self
                        .download_and_decrypt_buffer(&file_item.media.encrypt_query_param, &key)
                        .await?;
                    let safe_name = file_item.file_name.replace(['/', '\\'], "_");
                    let file_path = destination_dir.join(if safe_name.is_empty() {
                        format!("file_{}_{}.bin", now_millis()?, index)
                    } else {
                        safe_name
                    });
                    tokio::fs::create_dir_all(destination_dir).await.map_err(|error| {
                        WeixinClientError::new(format!("创建附件目录失败: {error}"))
                    })?;
                    tokio::fs::write(&file_path, bytes).await.map_err(|error| {
                        WeixinClientError::new(format!("写入文件附件失败: {error}"))
                    })?;
                    attachments.push(DownloadedAttachment {
                        kind: "file".into(),
                        summary: format!("File: {}", file_path.display()),
                        path: file_path,
                    });
                }
            }
        }

        Ok(attachments)
    }

    async fn upload_media_to_weixin(
        &self,
        file_path: &Path,
        to_user_id: &str,
        media_type: MediaType,
    ) -> Result<UploadedMedia, WeixinClientError> {
        let plaintext = tokio::fs::read(file_path)
            .await
            .map_err(|error| WeixinClientError::new(format!("读取媒体文件失败: {error}")))?;
        let raw_size = plaintext.len();
        let raw_md5 = hex_md5(&plaintext);
        let file_size_ciphertext = aes_ecb_padded_size(raw_size);
        let filekey = hex_random(16)?;
        let aes_key = crate::client::random_bytes::<16>()?;
        let upload_url_response = self
            .get_upload_url(
                to_user_id,
                raw_size,
                &raw_md5,
                file_size_ciphertext,
                &filekey,
                media_type,
                &bytes_to_hex(&aes_key),
            )
            .await?;

        let upload_param = upload_url_response.upload_param.trim();
        if upload_param.is_empty() {
            return Err(WeixinClientError::new("getuploadurl 返回了空 upload_param"));
        }

        let ciphertext = encrypt_aes_ecb(&plaintext, &aes_key)?;
        let cdn_url = format!(
            "{}upload?encrypted_query_param={}&filekey={}",
            self.client.config().cdn_base_url,
            url_encode(upload_param),
            url_encode(&filekey)
        );

        let response =
            self.client.post_bytes(&cdn_url, ciphertext, "application/octet-stream").await?;

        if response.status().as_u16() >= 400 {
            let status = response.status().as_u16();
            let error_message = response
                .headers()
                .get("x-error-message")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("cdn upload {status}"));
            return Err(WeixinClientError::new(format!("CDN 上传失败: {error_message}"))
                .with_status_code(Some(status)));
        }

        let download_param = response
            .headers()
            .get("x-encrypted-param")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| WeixinClientError::new("CDN 上传响应缺少 x-encrypted-param"))?;

        Ok(UploadedMedia {
            download_encrypted_query_param: download_param,
            aes_key_hex: bytes_to_hex(&aes_key),
            file_size: raw_size,
            file_size_ciphertext,
        })
    }

    async fn get_upload_url(
        &self,
        to_user_id: &str,
        raw_size: usize,
        raw_md5: &str,
        file_size_ciphertext: usize,
        filekey: &str,
        media_type: MediaType,
        aes_key_hex: &str,
    ) -> Result<UploadUrlResponse, WeixinClientError> {
        let body = json!({
            "filekey": filekey,
            "media_type": media_type as i64,
            "to_user_id": to_user_id,
            "rawsize": raw_size,
            "rawfilemd5": raw_md5,
            "filesize": file_size_ciphertext,
            "no_need_thumb": true,
            "aeskey": aes_key_hex,
            "base_info": {},
        });

        let response = self
            .client
            .post_json::<UploadUrlResponse>(
                "ilink/bot/getuploadurl",
                &body,
                self.client.config().api_timeout,
                true,
            )
            .await?;
        assert_weixin_ok_response(
            &json!({
                "errcode": response.envelope.errcode,
                "ret": response.envelope.ret,
                "errmsg": response.envelope.errmsg,
            }),
            "getuploadurl",
        )?;
        Ok(response)
    }

    async fn download_and_decrypt_buffer(
        &self,
        encrypted_query_param: &str,
        key: &[u8],
    ) -> Result<Vec<u8>, WeixinClientError> {
        let url = format!(
            "{}download?encrypted_query_param={}",
            self.client.config().cdn_base_url,
            url_encode(encrypted_query_param)
        );
        let encrypted = self.client.get_bytes(&url).await?;
        decrypt_aes_ecb(&encrypted, key)
    }
}

fn mime_from_filename(file_path: &Path) -> &'static str {
    match file_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

fn guess_image_extension(buffer: &[u8]) -> &'static str {
    if buffer.starts_with(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]) {
        ".png"
    } else if buffer.starts_with(&[0xff, 0xd8, 0xff]) {
        ".jpg"
    } else if buffer.starts_with(b"GIF87a") || buffer.starts_with(b"GIF89a") {
        ".gif"
    } else if buffer.len() >= 12 && &buffer[0..4] == b"RIFF" && &buffer[8..12] == b"WEBP" {
        ".webp"
    } else if buffer.starts_with(b"BM") {
        ".bmp"
    } else {
        ".jpg"
    }
}

fn hex_md5(bytes: &[u8]) -> String {
    let digest = Md5::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn aes_ecb_padded_size(plaintext_size: usize) -> usize {
    ((plaintext_size + 1).div_ceil(16)) * 16
}

fn encrypt_aes_ecb(plaintext: &[u8], key: &[u8]) -> Result<Vec<u8>, WeixinClientError> {
    let mut buffer = vec![0_u8; plaintext.len() + 16];
    buffer[..plaintext.len()].copy_from_slice(plaintext);
    Encryptor::<Aes128>::new_from_slice(key)
        .map_err(|error| WeixinClientError::new(format!("初始化 AES 加密失败: {error}")))?
        .encrypt_padded_mut::<Pkcs7>(&mut buffer, plaintext.len())
        .map(|ciphertext| ciphertext.to_vec())
        .map_err(|error| WeixinClientError::new(format!("AES 加密失败: {error}")))
}

fn decrypt_aes_ecb(ciphertext: &[u8], key: &[u8]) -> Result<Vec<u8>, WeixinClientError> {
    let mut buffer = ciphertext.to_vec();
    Decryptor::<Aes128>::new_from_slice(key)
        .map_err(|error| WeixinClientError::new(format!("初始化 AES 解密失败: {error}")))?
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map(|plaintext| plaintext.to_vec())
        .map_err(|error| WeixinClientError::new(format!("AES 解密失败: {error}")))
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, WeixinClientError> {
    if !hex.len().is_multiple_of(2) {
        return Err(WeixinClientError::new("十六进制字符串长度非法"));
    }

    (0..hex.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&hex[index..index + 2], 16)
                .map_err(|error| WeixinClientError::new(format!("解析十六进制数据失败: {error}")))
        })
        .collect()
}
