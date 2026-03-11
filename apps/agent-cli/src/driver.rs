use std::{
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
};

use agent_core::LanguageModel;
use agent_runtime::{AgentRuntime, RuntimeError, RuntimeEvent, RuntimeSubscriberId};
use session_tape::SessionTapeError;

use crate::model::{BootstrapTools, CliModel};

pub type CliRuntime = AgentRuntime<CliModel, BootstrapTools>;

pub struct HandoffSummary {
    pub summary: String,
    pub next_steps: Vec<String>,
}

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
    TurnProcessed(DriverTurnResult),
    Finalized(Result<HandoffSummary, DriverError>),
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

pub fn finalize_runtime<M, T>(
    runtime: &mut AgentRuntime<M, T>,
    session_path: &Path,
) -> Result<HandoffSummary, DriverError>
where
    M: LanguageModel,
    T: agent_core::ToolExecutor,
{
    let handoff = runtime.handoff(
        "首个真实模型适配器已经接入",
        vec![
            "把统一工具规范映射到外部协议".into(),
            "推进 MCP 风格工具协议接入".into(),
            "为终端界面准备稳定事件流".into(),
        ],
    );
    runtime.tape().save_jsonl(session_path)?;
    Ok(HandoffSummary {
        summary: handoff.anchor.state.summary,
        next_steps: handoff.anchor.state.next_steps,
    })
}

pub fn spawn_driver(
    mut runtime: CliRuntime,
    subscriber: RuntimeSubscriberId,
    session_path: PathBuf,
) -> DriverHandle {
    let (command_sender, command_receiver) = mpsc::channel::<DriverCommand>();
    let (response_sender, response_receiver) = mpsc::channel::<DriverResponse>();

    thread::spawn(move || {
        while let Ok(command) = command_receiver.recv() {
            match command {
                DriverCommand::ProcessTurn(prompt) => {
                    let result = process_turn(&mut runtime, subscriber, prompt, &session_path);
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

pub fn poll_driver(driver: &mut DriverHandle) -> Result<Option<DriverTurnResult>, DriverError> {
    match driver.receiver.try_recv() {
        Ok(DriverResponse::TurnProcessed(result)) => Ok(Some(result)),
        Ok(DriverResponse::Finalized(_)) => Ok(None),
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Disconnected) => Err(DriverError::Io(io::Error::other("驱动线程已断开"))),
    }
}

pub fn finalize_driver(driver: &mut DriverHandle) -> Result<HandoffSummary, DriverError> {
    driver
        .sender
        .send(DriverCommand::Finalize)
        .map_err(|error| DriverError::Io(io::Error::other(error.to_string())))?;
    loop {
        match driver.receiver.recv() {
            Ok(DriverResponse::Finalized(result)) => return result,
            Ok(DriverResponse::TurnProcessed(_)) => continue,
            Err(error) => return Err(DriverError::Io(io::Error::other(error.to_string()))),
        }
    }
}
