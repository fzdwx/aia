#![allow(dead_code)]

use std::{
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
};

use agent_core::{LanguageModel, StreamEvent, ToolRegistry};
use agent_runtime::{AgentRuntime, RuntimeError, RuntimeEvent, RuntimeSubscriberId};
use session_tape::SessionTapeError;

use crate::model::CliModel;

pub type CliRuntime = AgentRuntime<CliModel, ToolRegistry>;

pub struct DriverHandle {
    sender: Sender<DriverCommand>,
    receiver: Receiver<DriverResponse>,
}

#[derive(Debug)]
pub enum DriverError {
    Io(io::Error),
    Runtime(RuntimeError),
    Session(SessionTapeError),
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Runtime(error) => write!(f, "{error}"),
            Self::Session(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DriverError {}

impl From<io::Error> for DriverError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<RuntimeError> for DriverError {
    fn from(value: RuntimeError) -> Self {
        Self::Runtime(value)
    }
}

impl From<SessionTapeError> for DriverError {
    fn from(value: SessionTapeError) -> Self {
        Self::Session(value)
    }
}

pub struct DriverTurnResult {
    pub events: Vec<RuntimeEvent>,
    pub turn_error: Option<DriverError>,
    pub persist_error: Option<DriverError>,
}

enum DriverCommand {
    ProcessTurn(String),
    Finalize,
}

enum DriverResponse {
    StreamDelta(StreamEvent),
    TurnProcessed(DriverTurnResult),
    Finalized(Result<(), DriverError>),
}

pub enum DriverPollResult {
    StreamDelta(StreamEvent),
    TurnCompleted(DriverTurnResult),
    Nothing,
}

pub fn process_turn<M, T>(
    runtime: &mut AgentRuntime<M, T>,
    subscriber: RuntimeSubscriberId,
    prompt: String,
    session_path: &Path,
) -> DriverTurnResult
where
    M: LanguageModel,
    T: agent_core::ToolExecutor,
{
    match runtime.handle_turn(prompt) {
        Ok(output) => {
            let events = runtime.collect_events(subscriber).unwrap_or_default();
            let persist_error =
                runtime.tape().save_jsonl(session_path).err().map(DriverError::from);
            let _ = output;
            DriverTurnResult { events, turn_error: None, persist_error }
        }
        Err(error) => {
            let events = runtime.collect_events(subscriber).unwrap_or_default();
            let persist_error =
                runtime.tape().save_jsonl(session_path).err().map(DriverError::from);
            DriverTurnResult { events, turn_error: Some(DriverError::from(error)), persist_error }
        }
    }
}

fn process_turn_streaming<M, T>(
    runtime: &mut AgentRuntime<M, T>,
    subscriber: RuntimeSubscriberId,
    prompt: String,
    session_path: &Path,
    on_delta: impl FnMut(StreamEvent),
) -> DriverTurnResult
where
    M: LanguageModel,
    T: agent_core::ToolExecutor,
{
    match runtime.handle_turn_streaming(prompt, on_delta) {
        Ok(output) => {
            let events = runtime.collect_events(subscriber).unwrap_or_default();
            let persist_error =
                runtime.tape().save_jsonl(session_path).err().map(DriverError::from);
            let _ = output;
            DriverTurnResult { events, turn_error: None, persist_error }
        }
        Err(error) => {
            let events = runtime.collect_events(subscriber).unwrap_or_default();
            let persist_error =
                runtime.tape().save_jsonl(session_path).err().map(DriverError::from);
            DriverTurnResult { events, turn_error: Some(DriverError::from(error)), persist_error }
        }
    }
}

pub fn finalize_runtime<M, T>(
    runtime: &mut AgentRuntime<M, T>,
    session_path: &Path,
) -> Result<(), DriverError>
where
    M: LanguageModel,
    T: agent_core::ToolExecutor,
{
    runtime.tape().save_jsonl(session_path)?;
    Ok(())
}

pub fn spawn_driver<M, T>(
    mut runtime: AgentRuntime<M, T>,
    subscriber: RuntimeSubscriberId,
    session_path: PathBuf,
) -> DriverHandle
where
    M: LanguageModel + Send + 'static,
    T: agent_core::ToolExecutor + Send + 'static,
{
    let (command_sender, command_receiver) = mpsc::channel::<DriverCommand>();
    let (response_sender, response_receiver) = mpsc::channel::<DriverResponse>();

    thread::spawn(move || {
        while let Ok(command) = command_receiver.recv() {
            match command {
                DriverCommand::ProcessTurn(prompt) => {
                    let sender = response_sender.clone();
                    let result = process_turn_streaming(
                        &mut runtime,
                        subscriber,
                        prompt,
                        &session_path,
                        |event| {
                            let _ = sender.send(DriverResponse::StreamDelta(event));
                        },
                    );
                    let _ = response_sender.send(DriverResponse::TurnProcessed(result));
                }
                DriverCommand::Finalize => {
                    let result = finalize_runtime(&mut runtime, &session_path);
                    let _ = response_sender.send(DriverResponse::Finalized(result));
                    break;
                }
            }
        }
    });

    DriverHandle { sender: command_sender, receiver: response_receiver }
}

pub fn submit_turn(driver: &mut DriverHandle, prompt: String) -> Result<(), DriverError> {
    driver
        .sender
        .send(DriverCommand::ProcessTurn(prompt))
        .map_err(|error| DriverError::Io(io::Error::other(error.to_string())))
}

pub fn poll_driver(driver: &mut DriverHandle) -> Result<DriverPollResult, DriverError> {
    match driver.receiver.try_recv() {
        Ok(DriverResponse::StreamDelta(event)) => Ok(DriverPollResult::StreamDelta(event)),
        Ok(DriverResponse::TurnProcessed(result)) => Ok(DriverPollResult::TurnCompleted(result)),
        Ok(DriverResponse::Finalized(_)) => Ok(DriverPollResult::Nothing),
        Err(TryRecvError::Empty) => Ok(DriverPollResult::Nothing),
        Err(TryRecvError::Disconnected) => Err(DriverError::Io(io::Error::other("驱动线程已断开"))),
    }
}

pub fn finalize_driver(driver: &mut DriverHandle) -> Result<(), DriverError> {
    driver
        .sender
        .send(DriverCommand::Finalize)
        .map_err(|error| DriverError::Io(io::Error::other(error.to_string())))?;
    loop {
        match driver.receiver.recv() {
            Ok(DriverResponse::Finalized(result)) => return result,
            Ok(DriverResponse::TurnProcessed(_)) | Ok(DriverResponse::StreamDelta(_)) => continue,
            Err(error) => return Err(DriverError::Io(io::Error::other(error.to_string()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use agent_core::{ModelDisposition, ModelIdentity};
    use agent_runtime::AgentRuntime;
    use session_tape::SessionTape;

    use crate::model::{BootstrapModel, BootstrapTools};

    use super::finalize_runtime;

    fn temp_file(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).expect("时间有效").as_nanos();
        std::env::temp_dir().join(format!("aia-driver-{name}-{suffix}.jsonl"))
    }

    #[test]
    fn finalize_runtime_只保存当前磁带而不自动生成交接() {
        let identity = ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced);
        let mut runtime = AgentRuntime::new(BootstrapModel, BootstrapTools, identity)
            .with_instructions("保持简洁");
        runtime.handle_turn("第一句").expect("运行成功");

        let session_path = temp_file("finalize");
        finalize_runtime(&mut runtime, &session_path).expect("收尾成功");

        let restored = SessionTape::load_jsonl_or_default(&session_path).expect("载入成功");
        assert!(restored.entries().iter().any(|e| e.as_message().is_some()));
        assert!(restored.latest_anchor().is_none());

        let _ = fs::remove_file(session_path);
    }
}
