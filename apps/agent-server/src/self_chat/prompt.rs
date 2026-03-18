use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::bootstrap::ServerInitError;

pub(crate) const SELF_SESSION_TITLE_PREFIX: &str = "Self evolution";

pub(crate) async fn load_self_prompt() -> Result<String, ServerInitError> {
    let workspace_root = std::env::current_dir()
        .map_err(|error| ServerInitError::new("workspace 根目录获取", error.to_string()))?;
    let self_path = workspace_root.join("docs/self.md");
    let content = tokio::fs::read_to_string(&self_path)
        .await
        .map_err(|error| ServerInitError::new("docs/self.md 读取", error.to_string()))?;
    Ok(build_self_prompt(&self_path, &content))
}

pub(crate) fn build_self_prompt(path: &Path, content: &str) -> String {
    format!(
        "请先完整阅读 `{}` 的内容，并把它当作当前自我进化对话的工作约束。不要复述整份文件，只需吸收它，然后直接开始本轮对话。\n\n```md\n{}\n```",
        path.display(),
        content.trim()
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
    use std::path::Path;

    use super::{SELF_SESSION_TITLE_PREFIX, build_self_prompt, build_self_session_title};

    #[test]
    fn self_prompt_wraps_docs_self_contents() {
        let prompt = build_self_prompt(Path::new("docs/self.md"), "hello self");
        assert!(prompt.contains("docs/self.md"));
        assert!(prompt.contains("hello self"));
        assert!(prompt.contains("直接开始本轮对话"));
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
