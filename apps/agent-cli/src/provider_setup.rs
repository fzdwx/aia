#![allow(dead_code)]

use std::io::{BufRead, Write};

use provider_registry::{ProviderKind, ProviderProfile, ProviderRegistry};

use crate::{errors::CliSetupError, model::ProviderLaunchChoice};

pub fn choose_provider_interactively<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    registry: &mut ProviderRegistry,
    store_path: &std::path::Path,
) -> Result<ProviderLaunchChoice, CliSetupError> {
    writeln!(writer, "aia provider 设置")?;

    loop {
        if registry.providers().is_empty() {
            writeln!(writer, "1) 创建 OpenAI provider")?;
            writeln!(writer, "2) 使用本地 bootstrap")?;
            let choice = prompt_line(reader, writer, "请选择 [1-2]", None)?;

            match choice.as_str() {
                "1" | "" => {
                    let profile = create_openai_provider(reader, writer)?;
                    registry.upsert(profile.clone());
                    registry.set_active(&profile.name)?;
                    registry.save(store_path)?;
                    return Ok(ProviderLaunchChoice::OpenAi(profile));
                }
                "2" => return Ok(ProviderLaunchChoice::Bootstrap),
                _ => writeln!(writer, "无效选项，请重试。")?,
            }

            continue;
        }

        writeln!(writer, "已保存 provider：")?;
        let active_name = registry.active_provider().map(|provider| provider.name.as_str());
        let default_choice = registry
            .providers()
            .iter()
            .enumerate()
            .find(|(_, provider)| active_name == Some(provider.name.as_str()))
            .map(|(index, _)| (index + 1).to_string());
        for (index, provider) in registry.providers().iter().enumerate() {
            let mark = if active_name == Some(provider.name.as_str()) { " *当前" } else { "" };
            writeln!(writer, "{}) {} ({}){}", index + 1, provider.name, provider.model, mark)?;
        }

        let create_index = registry.providers().len() + 1;
        let bootstrap_index = registry.providers().len() + 2;
        writeln!(writer, "{}) 创建新的 OpenAI provider", create_index)?;
        writeln!(writer, "{}) 使用本地 bootstrap", bootstrap_index)?;

        let choice = prompt_line(
            reader,
            writer,
            &format!("请选择 [1-{bootstrap_index}]"),
            default_choice.as_deref(),
        )?;

        let selected_index = choice
            .parse::<usize>()
            .map_err(|_| CliSetupError::Message("请输入有效的数字选项".into()))?;

        if (1..=registry.providers().len()).contains(&selected_index) {
            let profile = registry.providers()[selected_index - 1].clone();
            registry.set_active(&profile.name)?;
            registry.save(store_path)?;
            return Ok(ProviderLaunchChoice::OpenAi(profile));
        }

        if selected_index == create_index {
            let profile = create_openai_provider(reader, writer)?;
            registry.upsert(profile.clone());
            registry.set_active(&profile.name)?;
            registry.save(store_path)?;
            return Ok(ProviderLaunchChoice::OpenAi(profile));
        }

        if selected_index == bootstrap_index {
            return Ok(ProviderLaunchChoice::Bootstrap);
        }

        writeln!(writer, "无效选项，请重试。")?;
    }
}

fn create_openai_provider<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Result<ProviderProfile, CliSetupError> {
    let kind = prompt_provider_kind(reader, writer)?;
    let name = prompt_required_line(reader, writer, "provider 名称")?;
    let model = prompt_required_line(reader, writer, "模型名称")?;
    let api_key = prompt_required_line(reader, writer, "API Key")?;
    let base_url = prompt_line(reader, writer, "Base URL", Some("https://api.openai.com/v1"))?;

    Ok(match kind {
        ProviderKind::OpenAiResponses => {
            ProviderProfile::openai_responses(name, base_url, api_key, model)
        }
        ProviderKind::OpenAiChatCompletions => {
            ProviderProfile::openai_chat_completions(name, base_url, api_key, model)
        }
    })
}

fn prompt_provider_kind<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Result<ProviderKind, CliSetupError> {
    loop {
        writeln!(writer, "请选择协议：")?;
        writeln!(writer, "1) OpenAI Responses")?;
        writeln!(writer, "2) OpenAI 兼容 Chat Completions")?;
        let choice = prompt_line(reader, writer, "请选择 [1-2]", Some("1"))?;
        match choice.as_str() {
            "1" => return Ok(ProviderKind::OpenAiResponses),
            "2" => return Ok(ProviderKind::OpenAiChatCompletions),
            _ => writeln!(writer, "无效选项，请重试。")?,
        }
    }
}

fn prompt_required_line<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    label: &str,
) -> Result<String, CliSetupError> {
    loop {
        let value = prompt_line(reader, writer, label, None)?;
        if !value.is_empty() {
            return Ok(value);
        }

        writeln!(writer, "{label} 不能为空。")?;
    }
}

pub(crate) fn prompt_line<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    label: &str,
    default: Option<&str>,
) -> Result<String, CliSetupError> {
    if let Some(default) = default {
        write!(writer, "{label} [{default}]: ")?;
    } else {
        write!(writer, "{label}: ")?;
    }
    writer.flush()?;

    let mut line = String::new();
    reader.read_line(&mut line)?;
    let value = line.trim().to_string();

    if value.is_empty() {
        return Ok(default.unwrap_or("").to_string());
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        time::{SystemTime, UNIX_EPOCH},
    };

    use provider_registry::{ProviderRegistry, default_registry_path};
    use session_tape::default_session_path;

    use crate::model::ProviderLaunchChoice;

    use super::choose_provider_interactively;

    fn temp_file(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).expect("时间有效").as_nanos();
        std::env::temp_dir().join(format!("aia-agent-cli-{name}-{suffix}.json"))
    }

    #[test]
    fn provider_文件默认放在项目内隐藏目录() {
        assert_eq!(default_registry_path(), std::path::PathBuf::from(".aia/providers.json"));
    }

    #[test]
    fn 会话文件默认放在项目内隐藏目录() {
        assert_eq!(default_session_path(), std::path::PathBuf::from(".aia/session.jsonl"));
    }

    #[test]
    fn 首次启动可通过终端创建_openai_provider() {
        let path = temp_file("create-provider");
        let mut registry = ProviderRegistry::default();
        let input = b"1\n1\nmain\ngpt-4.1-mini\nsecret\n\n";
        let mut reader = Cursor::new(input.as_slice());
        let mut output = Vec::new();

        let selection =
            choose_provider_interactively(&mut reader, &mut output, &mut registry, &path)
                .expect("创建成功");

        assert_eq!(
            selection,
            ProviderLaunchChoice::OpenAi(provider_registry::ProviderProfile::openai_responses(
                "main",
                "https://api.openai.com/v1",
                "secret",
                "gpt-4.1-mini"
            ))
        );
        assert_eq!(registry.providers().len(), 1);
        assert_eq!(registry.active_provider().map(|provider| provider.name.as_str()), Some("main"));

        let restored = ProviderRegistry::load_or_default(&path).expect("载入成功");
        assert_eq!(restored.providers().len(), 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn 首次启动可通过终端创建_chat_completions_provider() {
        let path = temp_file("create-chat-provider");
        let mut registry = ProviderRegistry::default();
        let input = b"1\n2\ncompat\nminum-security-llm\nsecret\nhttp://127.0.0.1:8000/v1\n";
        let mut reader = Cursor::new(input.as_slice());
        let mut output = Vec::new();

        let selection =
            choose_provider_interactively(&mut reader, &mut output, &mut registry, &path)
                .expect("创建成功");

        assert_eq!(
            selection,
            ProviderLaunchChoice::OpenAi(
                provider_registry::ProviderProfile::openai_chat_completions(
                    "compat",
                    "http://127.0.0.1:8000/v1",
                    "secret",
                    "minum-security-llm"
                )
            )
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn 已有_provider_时可直接选择已保存项() {
        let path = temp_file("select-provider");
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider_registry::ProviderProfile::openai_responses(
            "main",
            "https://api.openai.com/v1",
            "secret",
            "gpt-4.1-mini",
        ));
        registry.save(&path).expect("保存成功");

        let input = b"1\n";
        let mut reader = Cursor::new(input.as_slice());
        let mut output = Vec::new();

        let selection =
            choose_provider_interactively(&mut reader, &mut output, &mut registry, &path)
                .expect("选择成功");

        assert_eq!(
            selection,
            ProviderLaunchChoice::OpenAi(provider_registry::ProviderProfile::openai_responses(
                "main",
                "https://api.openai.com/v1",
                "secret",
                "gpt-4.1-mini"
            ))
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn 已有活动_provider_时直接回车会选中当前项() {
        let path = temp_file("default-select-provider");
        let mut registry = ProviderRegistry::default();
        registry.upsert(provider_registry::ProviderProfile::openai_responses(
            "main",
            "https://api.openai.com/v1",
            "secret",
            "gpt-4.1-mini",
        ));
        registry.set_active("main").expect("设置活动项成功");
        registry.save(&path).expect("保存成功");

        let input = b"\n";
        let mut reader = Cursor::new(input.as_slice());
        let mut output = Vec::new();

        let selection =
            choose_provider_interactively(&mut reader, &mut output, &mut registry, &path)
                .expect("选择成功");

        assert_eq!(
            selection,
            ProviderLaunchChoice::OpenAi(provider_registry::ProviderProfile::openai_responses(
                "main",
                "https://api.openai.com/v1",
                "secret",
                "gpt-4.1-mini"
            ))
        );
        let _ = fs::remove_file(path);
    }
}
