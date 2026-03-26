use std::{
    error::Error,
    fs,
    path::PathBuf,
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use agent_core::{AbortSignal, Tool, ToolCall, ToolExecutionContext};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};

use crate::exa::parse_exa_response;

use super::{
    EMPTY_RESULT_MESSAGE, WebSearchLivecrawl, WebSearchOutcome, WebSearchTool, WebSearchType,
    run_websearch_request,
};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Result<Self, Box<dyn Error>> {
        let unique =
            SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos())?;
        let path = std::env::temp_dir()
            .join(format!("aia-builtin-websearch-tests-{}-{unique}", process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn websearch_tool_definition_mentions_exa_and_current_year() {
    let definition = WebSearchTool.definition();

    assert!(definition.description.contains("Exa AI"));
    assert!(definition.description.contains("2026"));
    assert_eq!(definition.parameters["properties"]["numResults"]["default"], 8);
}

#[test]
fn parse_websearch_response_extracts_first_sse_payload_text() -> Result<(), Box<dyn Error>> {
    let body = concat!(
        "event: message\n",
        "data: {\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"latest result\"}]}}\n\n",
        "data: {\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"second result\"}]}}\n",
    );

    let parsed = parse_exa_response(body)?;
    assert_eq!(parsed.as_deref(), Some("latest result"));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn websearch_tool_returns_aborted_result_when_signal_is_pre_cancelled()
-> Result<(), Box<dyn Error>> {
    let dir = TestDir::new()?;
    let abort = AbortSignal::new();
    abort.abort();

    let tool = WebSearchTool;
    let call = ToolCall::new("websearch").with_arguments_value(serde_json::json!({
        "query": "AI news 2026"
    }));
    let result = tool
        .call(
            &call,
            &mut |_| {},
            &ToolExecutionContext {
                run_id: "test-run".into(),
                session_id: None,
                workspace_root: Some(dir.path().to_path_buf()),
                abort,
                runtime: None,
                runtime_host: None,
            },
        )
        .await
        .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

    assert_eq!(result.content, "[aborted]");
    let details = result.details.ok_or("websearch aborted result should include details")?;
    assert_eq!(details["aborted"], true);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn run_websearch_request_posts_mcp_payload_and_returns_context() -> Result<(), Box<dyn Error>>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (request_tx, request_rx) = oneshot::channel::<String>();
    let response_body = concat!(
        "event: message\n",
        "data: {\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"search context payload\"}]}}\n\n"
    );

    tokio::spawn(async move {
        let _ = serve_single_request(listener, response_body, request_tx).await;
    });

    let client = reqwest::Client::builder().build()?;
    let result = run_websearch_request(
        &client,
        &format!("http://{}", address),
        "AI news 2026",
        5,
        WebSearchLivecrawl::Preferred,
        WebSearchType::Deep,
        Some(9000),
        AbortSignal::new(),
    )
    .await?;

    let request = request_rx.await?;
    assert!(request.starts_with("POST /mcp HTTP/1.1"));
    assert!(request.contains("web_search_exa"));
    assert!(request.contains("\"query\":\"AI news 2026\""));
    assert!(request.contains("\"numResults\":5"));
    assert!(request.contains("\"livecrawl\":\"preferred\""));
    assert!(request.contains("\"type\":\"deep\""));
    assert!(request.contains("\"contextMaxCharacters\":9000"));

    match result {
        WebSearchOutcome::Completed(content) => assert_eq!(content, "search context payload"),
        WebSearchOutcome::Cancelled => {
            return Err("websearch request should not be cancelled".into());
        }
    }

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn run_websearch_request_falls_back_when_response_has_no_context()
-> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (request_tx, _request_rx) = oneshot::channel::<String>();
    let response_body = concat!("event: message\n", "data: {\"result\":{\"content\":[]}}\n\n");

    tokio::spawn(async move {
        let _ = serve_single_request(listener, response_body, request_tx).await;
    });

    let client = reqwest::Client::builder().build()?;
    let result = run_websearch_request(
        &client,
        &format!("http://{}", address),
        "latest AI news 2026",
        8,
        WebSearchLivecrawl::Fallback,
        WebSearchType::Auto,
        None,
        AbortSignal::new(),
    )
    .await?;

    match result {
        WebSearchOutcome::Completed(content) => assert_eq!(content, EMPTY_RESULT_MESSAGE),
        WebSearchOutcome::Cancelled => {
            return Err("websearch request should not be cancelled".into());
        }
    }

    Ok(())
}

async fn serve_single_request(
    listener: TcpListener,
    response_body: &str,
    request_tx: oneshot::Sender<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (mut stream, _) = listener.accept().await?;
    let request = read_http_request(&mut stream).await?;
    let _ = request_tx.send(request);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn read_http_request(
    stream: &mut tokio::net::TcpStream,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let mut header_end = None;
    let mut content_length = 0_usize;

    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);

        if header_end.is_none()
            && let Some(end) = find_subsequence(&buffer, b"\r\n\r\n")
        {
            header_end = Some(end + 4);
            content_length = parse_content_length(&buffer[..end + 4]);
        }

        if let Some(end) = header_end
            && buffer.len() >= end + content_length
        {
            break;
        }
    }

    Ok(String::from_utf8(buffer)?)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn parse_content_length(headers: &[u8]) -> usize {
    let headers = String::from_utf8_lossy(headers);
    for line in headers.lines() {
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            return value.trim().parse::<usize>().unwrap_or(0);
        }
    }
    0
}
