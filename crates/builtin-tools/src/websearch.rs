use std::time::Duration;

use agent_core::{
    AbortSignal, CoreError, Tool, ToolCall, ToolDefinition, ToolExecutionContext, ToolOutputDelta,
    ToolResult,
};
use agent_prompts::tool_descriptions::websearch_tool_description;
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

pub struct WebSearchTool;

const CURRENT_YEAR: &str = "2026";
const DEFAULT_NUM_RESULTS: usize = 8;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);
const EMPTY_RESULT_MESSAGE: &str = "No search results found. Please try a different query.";

pub(crate) enum WebSearchOutcome {
    Completed(String),
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum WebSearchLivecrawl {
    Fallback,
    Preferred,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum WebSearchType {
    Auto,
    Fast,
    Deep,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct WebSearchToolArgs {
    query: String,
    #[serde(rename = "numResults", alias = "num_results", default = "default_num_results")]
    pub(crate) num_results: usize,
    #[serde(default)]
    pub(crate) livecrawl: Option<WebSearchLivecrawl>,
    #[serde(rename = "type", default)]
    pub(crate) search_type: Option<WebSearchType>,
    #[serde(rename = "contextMaxCharacters", alias = "context_max_characters", default)]
    pub(crate) context_max_characters: Option<usize>,
}

#[derive(Serialize)]
struct McpSearchRequest<'a> {
    jsonrpc: &'static str,
    id: u8,
    method: &'static str,
    params: McpSearchRequestParams<'a>,
}

#[derive(Serialize)]
struct McpSearchRequestParams<'a> {
    name: &'static str,
    arguments: McpSearchRequestArguments<'a>,
}

#[derive(Serialize)]
struct McpSearchRequestArguments<'a> {
    query: &'a str,
    #[serde(rename = "numResults")]
    num_results: usize,
    livecrawl: WebSearchLivecrawl,
    #[serde(rename = "type")]
    search_type: WebSearchType,
    #[serde(rename = "contextMaxCharacters", skip_serializing_if = "Option::is_none")]
    context_max_characters: Option<usize>,
}

fn default_num_results() -> usize {
    DEFAULT_NUM_RESULTS
}

fn websearch_description() -> String {
    websearch_tool_description().replace("{{year}}", CURRENT_YEAR)
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), websearch_description()).with_parameters_value(
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Web search query. When searching for recent information or current events, include the current year 2026 in the query."
                    },
                    "numResults": {
                        "type": "integer",
                        "description": "Number of search results to return (default: 8)",
                        "minimum": 1,
                        "default": DEFAULT_NUM_RESULTS
                    },
                    "livecrawl": {
                        "type": "string",
                        "enum": ["fallback", "preferred"],
                        "description": "Live crawl mode - 'fallback': use live crawling as backup if cached content unavailable, 'preferred': prioritize live crawling (default: 'fallback')"
                    },
                    "type": {
                        "type": "string",
                        "enum": ["auto", "fast", "deep"],
                        "description": "Search type - 'auto': balanced search (default), 'fast': quick results, 'deep': comprehensive search"
                    },
                    "contextMaxCharacters": {
                        "type": "integer",
                        "description": "Maximum characters for context string optimized for LLMs (default: 10000)",
                        "minimum": 1
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
    ) -> Result<ToolResult, CoreError> {
        let args: WebSearchToolArgs = call.parse_arguments()?;
        let query = args.query.trim();
        if query.is_empty() {
            return Err(CoreError::new("missing required argument: query"));
        }
        if args.num_results == 0 {
            return Err(CoreError::new("numResults must be at least 1"));
        }
        if matches!(args.context_max_characters, Some(0)) {
            return Err(CoreError::new("contextMaxCharacters must be at least 1"));
        }

        let livecrawl = args.livecrawl.unwrap_or(WebSearchLivecrawl::Fallback);
        let search_type = args.search_type.unwrap_or(WebSearchType::Auto);

        if context.abort.is_aborted() {
            return Ok(ToolResult::from_call(call, "[aborted]").with_details(serde_json::json!({
                "query": query,
                "numResults": args.num_results,
                "livecrawl": livecrawl,
                "type": search_type,
                "contextMaxCharacters": args.context_max_characters,
                "aborted": true,
            })));
        }

        let client = Client::builder().timeout(REQUEST_TIMEOUT).build().map_err(|error| {
            CoreError::new(format!("failed to build websearch client: {error}"))
        })?;

        match run_websearch_request(
            &client,
            API_BASE_URL,
            query,
            args.num_results,
            livecrawl,
            search_type,
            args.context_max_characters,
            context.abort.clone(),
        )
        .await?
        {
            WebSearchOutcome::Completed(content) => {
                let result_found = content != EMPTY_RESULT_MESSAGE;
                Ok(ToolResult::from_call(call, content).with_details(serde_json::json!({
                    "query": query,
                    "numResults": args.num_results,
                    "livecrawl": livecrawl,
                    "type": search_type,
                    "contextMaxCharacters": args.context_max_characters,
                    "result_found": result_found,
                    "provider": "exa",
                })))
            }
            WebSearchOutcome::Cancelled => Ok(ToolResult::from_call(call, "[aborted]")
                .with_details(serde_json::json!({
                    "query": query,
                    "numResults": args.num_results,
                    "livecrawl": livecrawl,
                    "type": search_type,
                    "contextMaxCharacters": args.context_max_characters,
                    "aborted": true,
                }))),
        }
    }
}

pub(crate) async fn run_websearch_request(
    client: &Client,
    base_url: &str,
    query: &str,
    num_results: usize,
    livecrawl: WebSearchLivecrawl,
    search_type: WebSearchType,
    context_max_characters: Option<usize>,
    abort: AbortSignal,
) -> Result<WebSearchOutcome, CoreError> {
    let request_body = McpSearchRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "tools/call",
        params: McpSearchRequestParams {
            name: "web_search_exa",
            arguments: McpSearchRequestArguments {
                query,
                num_results,
                livecrawl,
                search_type,
                context_max_characters,
            },
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
            map_request_error(error, "Search request timed out", "websearch request failed")
        })?,
        RequestRace::Cancelled => return Ok(WebSearchOutcome::Cancelled),
    };

    let status = response.status();
    let body = match race_abort(response.text(), abort).await {
        RequestRace::Completed(result) => result.map_err(|error| {
            map_request_error(error, "Search request timed out", "websearch response read failed")
        })?,
        RequestRace::Cancelled => return Ok(WebSearchOutcome::Cancelled),
    };

    if !status.is_success() {
        return Err(CoreError::new(format!(
            "websearch request failed: POST {url} -> {status} {body}"
        )));
    }

    let content = parse_exa_response(&body)?
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| EMPTY_RESULT_MESSAGE.to_string());
    Ok(WebSearchOutcome::Completed(content))
}

#[cfg(test)]
#[path = "../tests/websearch/mod.rs"]
mod tests;
