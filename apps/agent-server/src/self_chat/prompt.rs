use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const SELF_SESSION_TITLE_PREFIX: &str = "Self evolution";
#[cfg(test)]
const EMBEDDED_SELF_PATH: &str = "docs/self.md";
const EMBEDDED_SELF_CONTENT: &str = include_str!("../../../../docs/self.md");

pub(crate) fn build_self_chat_system_prompt() -> String {
    EMBEDDED_SELF_CONTENT.trim().to_string()
}

pub(crate) fn build_initial_self_message(startup_task: Option<&str>) -> String {
    let trimmed_task = startup_task.map(str::trim).filter(|task| !task.is_empty());
    match trimmed_task {
        Some(task) => format!(
            "开始本轮 wake。优先处理这项任务：\n{task}\n\n按 system prompt 直接行动，不要先复述规则。"
        ),
        None => "开始本轮 wake。按 system prompt 直接行动，不要先复述规则。".to_string(),
    }
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
