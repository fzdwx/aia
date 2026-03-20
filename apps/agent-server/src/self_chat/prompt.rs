use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const SELF_SESSION_TITLE_PREFIX: &str = "Self evolution";
const EMBEDDED_SELF_PATH: &str = "docs/self.md";
const EMBEDDED_SELF_CONTENT: &str = include_str!("../../../../docs/self.md");

pub(crate) fn build_initial_self_prompt(startup_task: Option<&str>) -> String {
    let task_section = startup_task
        .map(str::trim)
        .filter(|task| !task.is_empty())
        .map(|task| {
            format!(
                "\n\n本次启动附加任务（来自用户启动参数）：\n{task}\n\n请在遵守上述约束的前提下，优先完成这项任务。"
            )
        })
        .unwrap_or_default();

    format!(
        "以下内容来自编译期内嵌的 `{EMBEDDED_SELF_PATH}`。请先完整吸收，不要复述整份文件，只需按其中约束直接开始本轮对话。\n\n<docs-self-md>\n{}\n</docs-self-md>{task_section}",
        EMBEDDED_SELF_CONTENT.trim()
    )
}

pub(crate) fn build_self_session_title() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{SELF_SESSION_TITLE_PREFIX} {timestamp}")
}

#[cfg(test)]
#[path = "../../tests/self_chat/prompt/mod.rs"]
mod tests;
