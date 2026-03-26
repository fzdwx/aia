use std::{future::Future, path::Path};

use agent_core::{AbortSignal, Tool, ToolCall, ToolExecutionContext, ToolOutputStream};

use super::{ShellTool, execution::run_embedded_brush};

fn run_async<T>(future: impl Future<Output = T>) -> T {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should build");
    runtime.block_on(future)
}

#[test]
fn embedded_brush_runtime_executes_shell_command() {
    let mut deltas = Vec::new();
    let execution = run_async(run_embedded_brush(
        "printf 'ok'",
        Path::new("."),
        &AbortSignal::new(),
        &mut |delta| {
            deltas.push(delta);
        },
    ))
    .expect("embedded brush execution should succeed");

    assert_eq!(execution.stdout, "ok");
    assert_eq!(execution.stderr, "");
    assert_eq!(execution.exit_code, 0);

    let stdout = deltas
        .iter()
        .filter(|delta| matches!(delta.stream, ToolOutputStream::Stdout))
        .map(|delta| delta.text.as_str())
        .collect::<String>();
    let stderr = deltas
        .iter()
        .filter(|delta| matches!(delta.stream, ToolOutputStream::Stderr))
        .map(|delta| delta.text.as_str())
        .collect::<String>();

    assert_eq!(stdout, "ok");
    assert_eq!(stderr, "");
}

#[tokio::test(flavor = "current_thread")]
async fn shell_call_keeps_stdout_stderr_and_exit_code_in_details() {
    let tool = ShellTool;
    let call =
        ToolCall::new("shell").with_argument("command", "printf 'out'; printf 'err' >&2; exit 7");
    let context = ToolExecutionContext {
        run_id: "test-run".into(),
        session_id: None,
        workspace_root: Some(Path::new(".").to_path_buf()),
        abort: AbortSignal::new(),
        runtime: None,
    };
    let mut deltas = Vec::new();

    let result = tool
        .call(&call, &mut |delta| deltas.push(delta), &context)
        .await
        .expect("shell tool should return a result");

    let details = result.details.expect("shell result should include details");
    assert_eq!(details["stdout"], "out");
    assert_eq!(details["stderr"], "err");
    assert_eq!(details["exit_code"], 7);

    let stdout = deltas
        .iter()
        .filter(|delta| matches!(delta.stream, ToolOutputStream::Stdout))
        .map(|delta| delta.text.as_str())
        .collect::<String>();
    let stderr = deltas
        .iter()
        .filter(|delta| matches!(delta.stream, ToolOutputStream::Stderr))
        .map(|delta| delta.text.as_str())
        .collect::<String>();

    assert_eq!(stdout, "out");
    assert_eq!(stderr, "err");
}

#[test]
fn embedded_brush_runtime_honors_abort_signal() {
    let abort = AbortSignal::new();
    let cancel = abort.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        cancel.abort();
    });

    let mut deltas = Vec::new();
    let execution = run_async(run_embedded_brush(
        "sleep 5; printf 'done'",
        Path::new("."),
        &abort,
        &mut |delta| deltas.push(delta),
    ))
    .expect("embedded brush should return after abort");

    assert_eq!(execution.exit_code, 130);
    let stdout = deltas
        .iter()
        .filter(|delta| matches!(delta.stream, ToolOutputStream::Stdout))
        .map(|delta| delta.text.as_str())
        .collect::<String>();
    assert!(!stdout.contains("done"));
}
