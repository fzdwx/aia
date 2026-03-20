use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use agent_prompts::SystemPromptConfig;
use agent_runtime::RuntimeHooks;

use super::{ServerBootstrapOptions, bootstrap_state_with_options};

fn run_async<T>(future: impl Future<Output = T>) -> T {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should build");
    runtime.block_on(future)
}

fn temp_root(name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    std::env::temp_dir().join(format!("aia-bootstrap-{name}-{suffix}"))
}

#[test]
fn bootstrap_state_with_options_applies_embedded_runtime_customization() {
    let root = temp_root("options");
    std::fs::create_dir_all(&root).expect("temp root should exist");
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));

    run_async(async {
        let hooks = RuntimeHooks::default().on_before_agent_start({
            let seen = seen.clone();
            move |event| {
                seen.lock().expect("test mutex should lock").push((
                    event.user_agent.clone().unwrap_or_default(),
                    event.instructions.clone().unwrap_or_default(),
                ));
                Ok(())
            }
        });

        let state = bootstrap_state_with_options(
            ServerBootstrapOptions::default()
                .with_registry_path(root.join("providers.json"))
                .with_workspace_root(root.clone())
                .with_user_agent("embed-test/1.0")
                .with_system_prompt(
                    SystemPromptConfig::default()
                        .with_custom_prompt("你是嵌入式客户端代理。")
                        .with_append_section("嵌入方附加约束"),
                )
                .with_runtime_hooks(hooks),
        )
        .await
        .expect("bootstrap should succeed");

        let session = state
            .session_manager
            .create_session(Some("Embedded client".into()))
            .await
            .expect("session should be created");
        let _ = state
            .session_manager
            .submit_turn(session.id.clone(), "hello".into())
            .await
            .expect("turn should be accepted");

        for _ in 0..200 {
            let history = state
                .session_manager
                .get_history(session.id.clone())
                .await
                .expect("history should be readable");
            if !history.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    });

    let seen = seen.lock().expect("test mutex should lock");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "embed-test/1.0");
    assert!(seen[0].1.contains("你是嵌入式客户端代理。"));
    assert!(seen[0].1.contains("嵌入方附加约束"));
    assert!(seen[0].1.contains("Context Contract"));

    let _ = std::fs::remove_dir_all(root);
}
