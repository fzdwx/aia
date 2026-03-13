use std::{env, error::Error, io};

use agent_runtime::AgentRuntime;
use builtin_tools::build_tool_registry;
use provider_registry::ProviderRegistry;
use session_tape::{SessionProviderBinding, SessionTape, default_session_path};

use crate::{
    errors::CliLoopError,
    loop_driver::run_agent_loop,
    model::{ProviderLaunchChoice, build_model_from_selection},
};

const CLI_DEFAULT_MAX_TURN_STEPS: usize = 50;
const CLI_DEFAULT_MAX_TOOL_CALLS_PER_TURN: usize = 50;

pub fn run() -> Result<(), Box<dyn Error>> {
    let prompt_seed = {
        let args = env::args().skip(1).collect::<Vec<_>>();
        if args.is_empty() { None } else { Some(args.join(" ")) }
    };

    let store_path = provider_registry::default_registry_path();
    let session_path = default_session_path();
    let registry = ProviderRegistry::load_or_default(&store_path)?;
    let tape = SessionTape::load_jsonl_or_default(&session_path)?;
    let selection = choose_non_interactive_provider(&registry, &tape);
    let (identity, model) = build_model_from_selection(selection)?;
    let tools = build_tool_registry();
    let current_model_provider = identity.provider.clone();
    let current_model_name = identity.name.clone();
    let mut runtime = AgentRuntime::with_tape(model, tools, identity, tape)
        .with_instructions("你是 aia 的起步代理。优先给出结构化、可继续落地的答案。")
        .with_max_turn_steps(CLI_DEFAULT_MAX_TURN_STEPS)
        .with_max_tool_calls_per_turn(CLI_DEFAULT_MAX_TOOL_CALLS_PER_TURN);
    runtime.disable_tool("handoff_session");

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    run_agent_loop(
        &mut reader,
        &mut writer,
        runtime,
        &session_path,
        Some(prompt_seed.unwrap_or_else(|| "描述一下 aia 的下一步".to_string())),
        &current_model_provider,
        &current_model_name,
    )
    .map_err(boxed_loop_error)?;

    Ok(())
}

fn boxed_loop_error(error: CliLoopError) -> Box<dyn Error> {
    Box::new(error)
}

fn choose_non_interactive_provider(
    registry: &ProviderRegistry,
    tape: &SessionTape,
) -> ProviderLaunchChoice {
    if let Some(binding) = tape.latest_provider_binding() {
        match binding {
            SessionProviderBinding::Bootstrap => return ProviderLaunchChoice::Bootstrap,
            SessionProviderBinding::Provider { name, model, base_url, protocol } => {
                if let Some(profile) = registry.providers().iter().find(|provider| {
                    provider.name == name
                        && provider.model == model
                        && provider.base_url == base_url
                        && provider.kind.protocol_name() == protocol.as_str()
                }) {
                    return ProviderLaunchChoice::OpenAi(profile.clone());
                }
            }
        }
    }

    registry
        .active_provider()
        .cloned()
        .map(ProviderLaunchChoice::OpenAi)
        .unwrap_or(ProviderLaunchChoice::Bootstrap)
}

#[cfg(test)]
mod tests {
    use provider_registry::{ProviderKind, ProviderProfile, ProviderRegistry};
    use session_tape::{SessionProviderBinding, SessionTape};

    use crate::model::ProviderLaunchChoice;

    use super::choose_non_interactive_provider;

    #[test]
    fn 非终端环境优先使用活动_provider() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(ProviderProfile::openai_responses(
            "main",
            "https://api.openai.com/v1",
            "secret",
            "gpt-4.1-mini",
        ));
        registry.set_active("main").expect("设置成功");
        let tape = SessionTape::new();

        let selection = choose_non_interactive_provider(&registry, &tape);

        assert!(
            matches!(selection, ProviderLaunchChoice::OpenAi(profile) if profile.name == "main")
        );
    }

    #[test]
    fn 非终端环境无_provider_时回退_bootstrap() {
        let registry = ProviderRegistry::default();
        let tape = SessionTape::new();

        assert_eq!(
            choose_non_interactive_provider(&registry, &tape),
            ProviderLaunchChoice::Bootstrap
        );
    }

    #[test]
    fn 非终端环境优先使用会话里记住的_provider() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(ProviderProfile::openai_responses(
            "main",
            "https://api.openai.com/v1",
            "secret",
            "gpt-4.1-mini",
        ));
        let mut tape = SessionTape::new();
        tape.bind_provider(SessionProviderBinding::Provider {
            name: "main".into(),
            model: "gpt-4.1-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
            protocol: "openai-responses".into(),
        });

        let selection = choose_non_interactive_provider(&registry, &tape);

        assert!(
            matches!(selection, ProviderLaunchChoice::OpenAi(profile) if profile.name == "main")
        );
    }

    #[test]
    fn 非终端环境会遵循会话里记住的_bootstrap() {
        let registry = ProviderRegistry::default();
        let mut tape = SessionTape::new();
        tape.bind_provider(SessionProviderBinding::Bootstrap);

        assert_eq!(
            choose_non_interactive_provider(&registry, &tape),
            ProviderLaunchChoice::Bootstrap
        );
    }

    #[test]
    fn 非终端环境会区分同地址同模型的不同协议_provider() {
        let mut registry = ProviderRegistry::default();
        registry.upsert(ProviderProfile::openai_responses(
            "resp",
            "http://127.0.0.1:8000/v1",
            "secret",
            "minum-security-llm",
        ));
        registry.upsert(ProviderProfile::openai_chat_completions(
            "compat",
            "http://127.0.0.1:8000/v1",
            "secret",
            "minum-security-llm",
        ));
        let mut tape = SessionTape::new();
        tape.bind_provider(SessionProviderBinding::Provider {
            name: "compat".into(),
            model: "minum-security-llm".into(),
            base_url: "http://127.0.0.1:8000/v1".into(),
            protocol: ProviderKind::OpenAiChatCompletions.protocol_name().into(),
        });

        let selection = choose_non_interactive_provider(&registry, &tape);

        assert!(matches!(
            selection,
            ProviderLaunchChoice::OpenAi(profile)
                if profile.name == "compat"
                    && profile.kind == ProviderKind::OpenAiChatCompletions
        ));
    }
}
