use std::time::Duration;

use agent_core::{
    AbortSignal, CoreError, Tool, ToolCall, ToolCallOutcome, ToolDefinition, ToolExecutionContext,
    ToolOutputDelta, ToolResult,
};
use agent_prompts::tool_descriptions::codesearch_tool_description;
use async_trait::async_trait;
use reqwest::{
    Client,
    header::{ACCEPT, CONTENT_TYPE},
};
use serde::{Deserialize, Serialize};

use crate::exa::{
    API_BASE_URL, API_MCP_ENDPOINT, RequestRace, endpoint_url, map_request_error,
    parse_exa_response, race_abort,
};

pub struct CodeSearchTool;

const DEFAULT_TOKENS_NUM: usize = 5000;
const MIN_TOKENS_NUM: usize = 1000;
const MAX_TOKENS_NUM: usize = 50000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const EMPTY_RESULT_MESSAGE: &str = "No code snippets or documentation found. Please try a different query, be more specific about the library or programming concept, or check the spelling of framework names.";

pub(crate) enum CodeSearchOutcome {
    Completed(String),
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CodeSearchToolArgs {
    query: String,
    #[serde(rename = "tokensNum", alias = "tokens_num", default = "default_tokens_num")]
    pub(crate) tokens_num: usize,
}

#[derive(Serialize)]
struct McpCodeRequest<'a> {
    jsonrpc: &'static str,
    id: u8,
    method: &'static str,
    params: McpCodeRequestParams<'a>,
}

#[derive(Serialize)]
struct McpCodeRequestParams<'a> {
    name: &'static str,
    arguments: McpCodeRequestArguments<'a>,
}

#[derive(Serialize)]
struct McpCodeRequestArguments<'a> {
    query: &'a str,
    #[serde(rename = "tokensNum")]
    tokens_num: usize,
}

fn default_tokens_num() -> usize {
    DEFAULT_TOKENS_NUM
}

#[async_trait]
impl Tool for CodeSearchTool {
    fn name(&self) -> &str {
        "CodeSearch"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), codesearch_tool_description()).with_parameters_value(
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query to find relevant context for APIs, libraries, SDKs, and programming concepts. For example, 'React useState hook examples', 'Python pandas dataframe filtering', 'Express.js middleware', 'Next.js partial prerendering configuration'."
                    },
                    "tokensNum": {
                        "type": "integer",
                        "description": "Number of tokens to return (1000-50000). Default is 5000 tokens. Use lower values for focused queries and higher values for broader documentation context.",
                        "minimum": MIN_TOKENS_NUM,
                        "maximum": MAX_TOKENS_NUM,
                        "default": DEFAULT_TOKENS_NUM
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        )
    }

    async fn call(
        &self,
        call: &ToolCall,
        _output: &mut (dyn FnMut(ToolOutputDelta) + Send),
        context: &ToolExecutionContext,
    ) -> Result<ToolCallOutcome, CoreError> {
        let args: CodeSearchToolArgs = call.parse_arguments()?;
        let query = args.query.trim();
        if query.is_empty() {
            return Err(CoreError::new("missing required argument: query"));
        }
        if !(MIN_TOKENS_NUM..=MAX_TOKENS_NUM).contains(&args.tokens_num) {
            return Err(CoreError::new(format!(
                "tokensNum must be between {MIN_TOKENS_NUM} and {MAX_TOKENS_NUM}"
            )));
        }

        if context.abort.is_aborted() {
            return Ok(ToolCallOutcome::completed(
                ToolResult::from_call(call, "[aborted]").with_details(serde_json::json!({
                    "query": query,
                    "tokensNum": args.tokens_num,
                    "aborted": true,
                })),
            ));
        }

        let client = Client::builder().timeout(REQUEST_TIMEOUT).build().map_err(|error| {
            CoreError::new(format!("failed to build codesearch client: {error}"))
        })?;

        match run_codesearch_request(
            &client,
            API_BASE_URL,
            query,
            args.tokens_num,
            context.abort.clone(),
        )
        .await?
        {
            CodeSearchOutcome::Completed(content) => {
                let result_found = content != EMPTY_RESULT_MESSAGE;
                Ok(ToolCallOutcome::completed(ToolResult::from_call(call, content).with_details(
                    serde_json::json!({
                        "query": query,
                        "tokensNum": args.tokens_num,
                        "result_found": result_found,
                        "provider": "exa",
                    }),
                )))
            }
            CodeSearchOutcome::Cancelled => Ok(ToolCallOutcome::completed(
                ToolResult::from_call(call, "[aborted]").with_details(serde_json::json!({
                    "query": query,
                    "tokensNum": args.tokens_num,
                    "aborted": true,
                })),
            )),
        }
    }
}

pub(crate) async fn run_codesearch_request(
    client: &Client,
    base_url: &str,
    query: &str,
    tokens_num: usize,
    abort: AbortSignal,
) -> Result<CodeSearchOutcome, CoreError> {
    let request_body = McpCodeRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "tools/call",
        params: McpCodeRequestParams {
            name: "get_code_context_exa",
            arguments: McpCodeRequestArguments { query, tokens_num },
        },
    };
    let url = endpoint_url(base_url, API_MCP_ENDPOINT);

    let response = match race_abort(
        client
            .post(&url)
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .json(&request_body)
            .send(),
        abort.clone(),
    )
    .await
    {
        RequestRace::Completed(result) => result.map_err(|error| {
            map_request_error(error, "Code search request timed out", "codesearch request failed")
        })?,
        RequestRace::Cancelled => return Ok(CodeSearchOutcome::Cancelled),
    };

    let status = response.status();
    let body = match race_abort(response.text(), abort).await {
        RequestRace::Completed(result) => result.map_err(|error| {
            map_request_error(
                error,
                "Code search request timed out",
                "codesearch response read failed",
            )
        })?,
        RequestRace::Cancelled => return Ok(CodeSearchOutcome::Cancelled),
    };

    if !status.is_success() {
        return Err(CoreError::new(format!(
            "codesearch request failed: POST {url} -> {status} {body}"
        )));
    }

    let content = parse_exa_response(&body)?
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| EMPTY_RESULT_MESSAGE.to_string());
    Ok(CodeSearchOutcome::Completed(content))
}

#[cfg(test)]
#[path = "../tests/codesearch/mod.rs"]
mod tests;
