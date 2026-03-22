use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use serde_json::Value;
use weixin_client::{SendTextRequest, WeixinClient, WeixinClientConfig};

#[tokio::test(flavor = "current_thread")]
async fn sends_text_message_with_expected_headers_and_body() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("监听成功");
    let address = listener.local_addr().expect("读取地址成功");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("接受连接成功");
        let mut buffer = [0_u8; 8192];
        let size = stream.read(&mut buffer).expect("读取请求成功");
        let request_text = String::from_utf8_lossy(&buffer[..size]).to_string();

        assert!(request_text.starts_with("POST /ilink/bot/sendmessage HTTP/1.1"));
        assert!(
            request_text.contains("authorizationtype: ilink_bot_token\r\n")
                || request_text.to_lowercase().contains("authorizationtype: ilink_bot_token\r\n")
        );
        assert!(request_text.to_lowercase().contains("authorization: bearer bot-token\r\n"));
        assert!(
            request_text.contains("SKRouteTag: weixin-route\r\n")
                || request_text.to_lowercase().contains("skroutetag: weixin-route\r\n")
        );

        let body = request_text.split("\r\n\r\n").nth(1).expect("请求体");
        let payload: Value = serde_json::from_str(body).expect("解析 json 请求体");
        assert_eq!(payload["msg"]["to_user_id"], "wx-user-1");
        assert_eq!(payload["msg"]["context_token"], "ctx-1");
        assert_eq!(payload["msg"]["item_list"][0]["text_item"]["text"], "hello from rust");

        let response = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{\"errcode\":0}";
        stream.write_all(response.as_bytes()).expect("写回响应成功");
    });

    let client = WeixinClient::new(
        WeixinClientConfig::new(format!("http://{address}"), Some("bot-token"))
            .with_route_tag(Some("weixin-route")),
    )
    .expect("client 创建成功");

    let response = client
        .send_text(SendTextRequest::new("wx-user-1", "ctx-1", "hello from rust"))
        .await
        .expect("发送成功");

    handle.join().expect("服务线程退出");
    assert!(!response.message_id.is_empty());
}

#[test]
fn splits_text_prefers_newline_boundaries() {
    let text = format!("{}\n{}\n{}", "a".repeat(170), "b".repeat(170), "c".repeat(50));
    let chunks = weixin_client::WeixinClient::split_text_for_weixin(&text, 220);

    assert_eq!(
        chunks,
        vec![format!("{}\n", "a".repeat(170)), format!("{}\n", "b".repeat(170)), "c".repeat(50)]
    );
}
