use std::{
    io::{self, BufRead, Write},
    path::Path,
};

use agent_core::LanguageModel;
use agent_runtime::{
    AgentRuntime, RuntimeEvent, RuntimeSubscriberId, ToolInvocationOutcome, TurnLifecycle,
};

use crate::driver;
use crate::errors::{CliLoopError, CliSetupError};
use crate::provider_setup::prompt_line;

pub fn run_agent_loop<M, T, R, W>(
    reader: &mut R,
    writer: &mut W,
    mut runtime: AgentRuntime<M, T>,
    session_path: &Path,
    initial_prompt: Option<String>,
    current_model_provider: &str,
    current_model_name: &str,
) -> Result<(), CliLoopError>
where
    M: LanguageModel,
    T: agent_core::ToolExecutor,
    R: BufRead,
    W: Write,
{
    let subscriber = runtime.subscribe();
    writeln!(writer, "当前模型：{current_model_provider}/{current_model_name}")?;
    writeln!(writer, "输入内容后回车发送，输入 退出 / quit / exit 结束。")?;

    if let Some(prompt) = initial_prompt.filter(|value| !value.trim().is_empty()) {
        render_turn(writer, &mut runtime, subscriber, prompt, session_path)?;
    }

    loop {
        let input = prompt_line(reader, writer, "你", None).map_err(|error| match error {
            CliSetupError::Io(inner) => CliLoopError::Io(inner),
            other => CliLoopError::Io(io::Error::other(other.to_string())),
        })?;

        if input.is_empty() {
            writeln!(writer, "请输入非空内容，或输入 退出 结束。")?;
            continue;
        }

        if is_exit_command(&input) {
            writeln!(writer, "已退出 aia agent loop")?;
            break;
        }

        render_turn(writer, &mut runtime, subscriber, input, session_path)?;
    }

    driver::finalize_runtime(&mut runtime, session_path)?;

    Ok(())
}

fn render_turn<M, T, W>(
    writer: &mut W,
    runtime: &mut AgentRuntime<M, T>,
    subscriber: RuntimeSubscriberId,
    prompt: String,
    session_path: &Path,
) -> Result<(), CliLoopError>
where
    M: LanguageModel,
    T: agent_core::ToolExecutor,
    W: Write,
{
    let result = driver::process_turn(runtime, subscriber, prompt, session_path);
    render_events(writer, &result.events)?;
    if let Some(error) = result.persist_error {
        return Err(error.into());
    }
    if let Some(error) = result.turn_error {
        writeln!(writer, "[状态] 当前轮失败，但会话继续：{error}")?;
    }
    Ok(())
}

pub fn render_events<W: Write>(writer: &mut W, events: &[RuntimeEvent]) -> Result<(), io::Error> {
    let turn_events = events
        .iter()
        .filter_map(|event| match event {
            RuntimeEvent::TurnLifecycle { turn } => Some(turn),
            _ => None,
        })
        .collect::<Vec<_>>();

    if !turn_events.is_empty() {
        for turn in turn_events {
            render_turn_lifecycle(writer, turn)?;
        }
        return Ok(());
    }

    for event in events {
        match event {
            RuntimeEvent::UserMessage { content } => writeln!(writer, "[用户] {content}")?,
            RuntimeEvent::AssistantMessage { content } => writeln!(writer, "[助手] {content}")?,
            RuntimeEvent::ToolInvocation { call, outcome } => match outcome {
                ToolInvocationOutcome::Succeeded { result } => writeln!(
                    writer,
                    "[工具调用] {} #{} -> {}",
                    call.tool_name, call.invocation_id, result.content
                )?,
                ToolInvocationOutcome::Failed { message } => writeln!(
                    writer,
                    "[工具调用失败] {} #{} -> {}",
                    call.tool_name, call.invocation_id, message
                )?,
            },
            RuntimeEvent::TurnLifecycle { .. } => {}
            RuntimeEvent::TurnFailed { message } => writeln!(writer, "[失败] {message}")?,
        }
    }

    Ok(())
}

pub fn render_turn_lifecycle<W: Write>(
    writer: &mut W,
    turn: &TurnLifecycle,
) -> Result<(), io::Error> {
    writeln!(
        writer,
        "[轮次] {} ({} -> {}, 源条目: {:?})",
        turn.turn_id, turn.started_at_ms, turn.finished_at_ms, turn.source_entry_ids
    )?;
    writeln!(writer, "[用户] {}", turn.user_message)?;
    if let Some(thinking) = &turn.thinking {
        writeln!(writer, "[思考] {thinking}")?;
    }
    if let Some(assistant_message) = &turn.assistant_message {
        writeln!(writer, "[助手] {assistant_message}")?;
    }
    for invocation in &turn.tool_invocations {
        match &invocation.outcome {
            ToolInvocationOutcome::Succeeded { result } => writeln!(
                writer,
                "[工具调用] {} #{} -> {}",
                invocation.call.tool_name, invocation.call.invocation_id, result.content
            )?,
            ToolInvocationOutcome::Failed { message } => writeln!(
                writer,
                "[工具调用失败] {} #{} -> {}",
                invocation.call.tool_name, invocation.call.invocation_id, message
            )?,
        }
    }
    if let Some(failure_message) = &turn.failure_message {
        writeln!(writer, "[失败] {failure_message}")?;
    }
    Ok(())
}

pub fn is_exit_command(input: &str) -> bool {
    matches!(input.trim(), "退出" | "quit" | "exit")
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        time::{SystemTime, UNIX_EPOCH},
    };

    use agent_runtime::AgentRuntime;
    use session_tape::SessionTape;

    use crate::model::{BootstrapModel, BootstrapTools};

    use super::run_agent_loop;

    fn temp_file(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).expect("时间有效").as_nanos();
        std::env::temp_dir().join(format!("aia-agent-cli-{name}-{suffix}.json"))
    }

    #[test]
    fn agent_loop_会持续处理多轮输入() {
        let identity = agent_core::ModelIdentity::new(
            "local",
            "bootstrap",
            agent_core::ModelDisposition::Balanced,
        );
        let runtime = AgentRuntime::new(BootstrapModel, BootstrapTools, identity)
            .with_instructions("保持简洁");
        let mut reader = Cursor::new("第一句\n第二句\n退出\n".as_bytes());
        let mut writer = Vec::new();

        run_agent_loop(
            &mut reader,
            &mut writer,
            runtime,
            &temp_file("loop-session"),
            Some("初始问题".into()),
            "local",
            "bootstrap",
        )
        .expect("循环运行成功");

        let output = String::from_utf8(writer).expect("输出是有效 utf-8");
        assert!(output.contains("当前模型：local/bootstrap"));
        assert!(output.contains("[轮次] turn-"));
        assert!(output.contains("[用户] 初始问题"));
        assert!(output.contains("[助手] 收到需求：初始问题"));
        assert!(output.contains("[工具调用] search_code #tool-call-"));
        assert!(output.contains("起步阶段尚未接入真实工具执行器"));
        assert!(output.contains("[用户] 第一句"));
        assert!(output.contains("[助手] 收到需求：第一句"));
        assert!(output.contains("[用户] 第二句"));
        assert!(output.contains("[助手] 收到需求：第二句"));
    }

    #[test]
    fn agent_loop_会自动保存_session_jsonl() {
        let identity = agent_core::ModelIdentity::new(
            "local",
            "bootstrap",
            agent_core::ModelDisposition::Balanced,
        );
        let runtime = AgentRuntime::new(BootstrapModel, BootstrapTools, identity)
            .with_instructions("保持简洁");
        let session_path = temp_file("autosave-session");
        let mut reader = Cursor::new("第一句\n退出\n".as_bytes());
        let mut writer = Vec::new();

        run_agent_loop(
            &mut reader,
            &mut writer,
            runtime,
            &session_path,
            None,
            "local",
            "bootstrap",
        )
        .expect("循环运行成功");

        let restored = SessionTape::load_jsonl_or_default(&session_path).expect("载入会话成功");
        assert!(restored.entries().iter().any(|e| e.as_message().is_some()));
        assert!(restored.latest_anchor().is_none());

        let _ = fs::remove_file(session_path);
    }

    #[test]
    fn agent_loop_遇到退出指令会结束() {
        let identity = agent_core::ModelIdentity::new(
            "local",
            "bootstrap",
            agent_core::ModelDisposition::Balanced,
        );
        let runtime = AgentRuntime::new(BootstrapModel, BootstrapTools, identity)
            .with_instructions("保持简洁");
        let mut reader = Cursor::new("退出\n".as_bytes());
        let mut writer = Vec::new();

        run_agent_loop(
            &mut reader,
            &mut writer,
            runtime,
            &temp_file("exit-session"),
            None,
            "local",
            "bootstrap",
        )
        .expect("循环运行成功");

        let output = String::from_utf8(writer).expect("输出是有效 utf-8");
        assert!(output.contains("已退出 aia agent loop"));
        assert!(!output.contains("收到需求：退出"));
        assert!(!output.contains("交接摘要："));
    }

    #[test]
    fn agent_loop_失败时会输出失败事件且继续交互() {
        struct FailingCliModel;

        impl agent_core::LanguageModel for FailingCliModel {
            type Error = agent_core::CoreError;

            fn complete(
                &self,
                _request: agent_core::CompletionRequest,
            ) -> Result<agent_core::Completion, Self::Error> {
                Err(agent_core::CoreError::new("故意失败"))
            }
        }

        let identity = agent_core::ModelIdentity::new(
            "local",
            "failing",
            agent_core::ModelDisposition::Balanced,
        );
        let runtime = AgentRuntime::new(FailingCliModel, BootstrapTools, identity);
        let mut reader = Cursor::new("第一句\n退出\n".as_bytes());
        let mut writer = Vec::new();

        let result = run_agent_loop(
            &mut reader,
            &mut writer,
            runtime,
            &temp_file("failing-session"),
            None,
            "local",
            "failing",
        );

        assert!(result.is_ok());
        let output = String::from_utf8(writer).expect("输出是有效 utf-8");
        assert!(output.contains("当前模型：local/failing"));
        assert!(output.contains("[轮次] turn-"));
        assert!(output.contains("[用户] 第一句"));
        assert!(output.contains("[失败] 模型执行失败：故意失败"));
        assert!(output.contains("[状态] 当前轮失败，但会话继续：模型执行失败：故意失败"));
        assert!(output.contains("已退出 aia agent loop"));
    }
}
