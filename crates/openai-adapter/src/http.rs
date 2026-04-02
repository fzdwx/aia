use std::time::Duration;

use agent_core::{CompletionRequest, PromptCacheConfig};
use reqwest::{
    Client, RequestBuilder, StatusCode,
    header::{HeaderValue, USER_AGENT},
};
use serde_json::{Value, json};

use crate::OpenAiAdapterError;

pub(crate) fn validate_request_model(
    expected_model: &str,
    request: &CompletionRequest,
) -> Result<(), OpenAiAdapterError> {
    if request.model.name != expected_model {
        return Err(OpenAiAdapterError::new(format!(
            "模型标识不一致：请求为 {}，适配器配置为 {}",
            request.model.name, expected_model
        )));
    }

    Ok(())
}

pub(crate) fn endpoint_url(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), path.trim_start_matches('/'))
}

pub(crate) fn request_failure(
    endpoint_url: &str,
    status: StatusCode,
    body: &str,
) -> OpenAiAdapterError {
    OpenAiAdapterError::new(format!("请求失败：POST {endpoint_url} -> {status} {body}"))
        .with_status_code(Some(status.as_u16()))
        .with_response_body(Some(body.to_string()))
        .with_retryable(matches!(
            status,
            StatusCode::REQUEST_TIMEOUT
                | StatusCode::TOO_MANY_REQUESTS
                | StatusCode::INTERNAL_SERVER_ERROR
                | StatusCode::BAD_GATEWAY
                | StatusCode::SERVICE_UNAVAILABLE
                | StatusCode::GATEWAY_TIMEOUT
        ))
}

pub(crate) fn apply_user_agent(
    request: RequestBuilder,
    user_agent: Option<&str>,
) -> RequestBuilder {
    let Some(user_agent) = user_agent.filter(|value| !value.is_empty()) else {
        return request;
    };
    let Ok(value) = HeaderValue::from_str(user_agent) else {
        return request;
    };
    request.header(USER_AGENT, value)
}

pub(crate) fn http_client(request: &CompletionRequest) -> Result<Client, OpenAiAdapterError> {
    let mut builder = Client::builder();
    if let Some(timeout_ms) = request.timeout.as_ref().and_then(|timeout| timeout.read_timeout_ms) {
        let duration = Duration::from_millis(timeout_ms);
        // 使用 connect_timeout 而非全局 timeout，避免 streaming 请求被总时间限制断开。
        // streaming 只要有持续数据流就不应超时，connect_timeout 只限制建立连接的时间。
        builder = builder.connect_timeout(duration);
    }
    builder.build().map_err(|error| OpenAiAdapterError::new(error.to_string()))
}

pub(crate) fn apply_prompt_cache(body: &mut Value, prompt_cache: Option<&PromptCacheConfig>) {
    if let Some(prompt_cache) = prompt_cache {
        if let Some(key) = prompt_cache.key.as_ref().filter(|value| !value.is_empty()) {
            body["prompt_cache_key"] = json!(key);
        }
        if let Some(retention) = prompt_cache.retention.as_ref() {
            body["prompt_cache_retention"] = json!(retention.as_api_value());
        }
    }
}
