use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const SYSTEM_PROMPT: &str = r#"你是一个语音识别结果校正助手。你的任务是极其保守地修正语音识别错误。

规则：
1. 只修复明显的语音识别错误：
   - 中文谐音错误（如「配森」→「Python」）
   - 英文技术术语被错误转为中文（如「杰森」→「JSON」）
   - 同音字错误（如「工做」→「工作」）
2. 绝对不要：
   - 改写句子结构
   - 润色文字
   - 添加或删除任何内容
   - 改变原意
3. 如果输入看起来正确，必须原样返回
4. 保持原文的标点符号和格式

直接输出修正后的文字，不要有任何解释。"#;

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

/// Refine transcribed text using LLM
pub async fn refine_text(api_base: &str, api_key: &str, model: &str, text: &str) -> Result<String> {
    if text.trim().is_empty() {
        return Ok(text.to_string());
    }

    let client = Client::new();

    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: text.to_string(),
            },
        ],
        max_tokens: Some(4096),
    };

    let url = format!("{}/chat/completions", api_base);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(anyhow::anyhow!("LLM API error: {}", error_text));
    }

    let result: ChatResponse = response.json().await?;

    let refined = result
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_else(|| text.to_string());

    Ok(refined)
}

/// Test LLM connection
pub async fn test_connection(api_base: &str, api_key: &str, model: &str) -> Result<bool> {
    let client = Client::new();

    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: "test".to_string(),
        }],
        max_tokens: Some(5),
    };

    let url = format!("{}/chat/completions", api_base);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    Ok(response.status().is_success())
}
