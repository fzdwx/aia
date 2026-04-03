#[path = "../routes/diff/highlighting.rs"]
mod highlighting;

fn main() {
    let content = r#"switch (event.name) {
case "response.first_text_delta":
case "response.first_reasoning_delta":
return asString(attributes?.preview) ?? null
case "response.retrying": {
const attempt = attributes?.attempt
const maxAttempts = attributes?.max_attempts
const reason = asString(attributes?.reason)
const prefix =
typeof attempt === "number" && typeof maxAttempts === "number"
? `attempt ${attempt + 1}/${maxAttempts}`
: "retrying"
return reason ? `${prefix}: ${reason}` : prefix
}
case "response.tool_call_detected":
case "response.tool_call_started":
return asString(attributes?.tool_name) ?? null
}"#;

    for (index, line) in highlighting::highlight_document_lines(
        content,
        "apps/web/src/features/trace/lib/trace-timeline.ts",
        highlighting::DiffTheme::Dark,
    )
    .into_iter()
    .enumerate()
    {
        println!("{}\t{}", index + 1, line);
    }
}
