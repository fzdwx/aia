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
mod tests {
    use super::{
        EMBEDDED_SELF_PATH, SELF_SESSION_TITLE_PREFIX, build_initial_self_prompt,
        build_self_session_title,
    };

    #[test]
    fn self_prompt_embeds_self_contents() {
        let prompt = build_initial_self_prompt(None);
        assert!(prompt.contains(EMBEDDED_SELF_PATH));
        assert!(prompt.contains("<docs-self-md>"));
        assert!(prompt.contains("直接开始本轮对话"));
    }

    #[test]
    fn self_prompt_appends_startup_task() {
        let prompt = build_initial_self_prompt(Some("stabilize self chat boot flow"));
        assert!(prompt.contains("启动附加任务"));
        assert!(prompt.contains("stabilize self chat boot flow"));
    }

    #[test]
    fn self_session_title_includes_timestamp_suffix() {
        let title = build_self_session_title();
        assert!(title.starts_with(SELF_SESSION_TITLE_PREFIX));

        let suffix = title
            .strip_prefix(SELF_SESSION_TITLE_PREFIX)
            .expect("title should keep self prefix")
            .trim();
        assert!(!suffix.is_empty());
        assert!(suffix.parse::<u64>().is_ok());
    }
}
