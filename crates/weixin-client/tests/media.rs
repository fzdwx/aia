use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value;
use weixin_client::{SendMediaFileRequest, WeixinClient, WeixinClientConfig};

#[tokio::test(flavor = "current_thread")]
async fn uploads_image_and_sends_media_message() {
    let temp_dir = std::env::temp_dir();
    let image_path = temp_dir.join(format!("weixin-client-test-{}.png", std::process::id()));
    std::fs::write(&image_path, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3, 4])
        .expect("写入测试图片成功");

    let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
    let address = listener.local_addr().expect("读取地址成功");
    let uploads: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let uploads_handle = Arc::clone(&uploads);

    let handle = thread::spawn(move || {
        for round in 0..3 {
            let (mut stream, _) = listener.accept().expect("接受连接成功");
            let mut buffer = [0_u8; 16384];
            let size = stream.read(&mut buffer).expect("读取请求成功");
            let request = &buffer[..size];
            let request_text = String::from_utf8_lossy(request).to_string();

            match round {
                0 => {
                    assert!(request_text.starts_with("POST /ilink/bot/getuploadurl HTTP/1.1"));
                    let body = request_text.split("\r\n\r\n").nth(1).expect("请求体");
                    let payload: Value = serde_json::from_str(body).expect("upload json");
                    assert_eq!(payload["media_type"], 1);
                    let response = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{\"errcode\":0,\"upload_param\":\"upload-ticket\"}";
                    stream.write_all(response.as_bytes()).expect("写回响应成功");
                }
                1 => {
                    assert!(
                        request_text.starts_with(
                            "POST /upload?encrypted_query_param=upload-ticket&filekey="
                        )
                    );
                    let body = request.split(|window| window == &b'\r').collect::<Vec<_>>();
                    let raw = request_text
                        .split("\r\n\r\n")
                        .nth(1)
                        .unwrap_or_default()
                        .as_bytes()
                        .to_vec();
                    uploads_handle.lock().expect("锁成功").push(raw);
                    let response = "HTTP/1.1 200 OK\r\nconnection: close\r\nx-encrypted-param: download-ticket\r\n\r\n";
                    stream.write_all(response.as_bytes()).expect("写回响应成功");
                    let _ = body;
                }
                _ => {
                    assert!(request_text.starts_with("POST /ilink/bot/sendmessage HTTP/1.1"));
                    let body = request_text.split("\r\n\r\n").nth(1).expect("请求体");
                    let payload: Value = serde_json::from_str(body).expect("message json");
                    assert_eq!(payload["msg"]["item_list"][0]["type"], 2);
                    assert_eq!(
                        payload["msg"]["item_list"][0]["image_item"]["media"]["encrypt_query_param"],
                        "download-ticket"
                    );
                    let response = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{\"ret\":0}";
                    stream.write_all(response.as_bytes()).expect("写回响应成功");
                }
            }
        }
    });

    let client = WeixinClient::new(
        WeixinClientConfig::new(format!("http://{address}"), Some("bot-token"))
            .with_cdn_base_url(format!("http://{address}")),
    )
    .expect("client 创建成功");

    let response = client
        .media()
        .send_media_file(SendMediaFileRequest::new("wx-user-1", "ctx-1", &image_path))
        .await
        .expect("媒体发送成功");

    handle.join().expect("服务线程退出");
    assert!(!response.message_id.is_empty());
    assert_eq!(uploads.lock().expect("锁成功").len(), 1);

    let _ = std::fs::remove_file(&image_path);
}
