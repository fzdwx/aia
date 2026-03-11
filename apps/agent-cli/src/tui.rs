use std::{io, path::Path, time::Duration};

use agent_runtime::{AgentRuntime, RuntimeEvent, RuntimeSubscriberId, TurnLifecycle};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use provider_registry::{ProviderProfile, ProviderRegistry};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use session_tape::{SessionProviderBinding, SessionTape};

use crate::{
    errors::CliLoopError,
    loop_driver::{finalize_runtime, is_exit_command, try_process_turn},
    model::{BootstrapTools, CliModel, ProviderLaunchChoice, build_model_from_selection},
};

type CliRuntime = AgentRuntime<CliModel, BootstrapTools>;

pub fn run_tui_loop(
    mut registry: ProviderRegistry,
    store_path: &Path,
    tape: SessionTape,
    session_path: &Path,
    prompt_seed: Option<String>,
) -> Result<(), CliLoopError> {
    let (remembered_selection, startup_notice) = resolve_remembered_selection(&tape, &registry);
    let mut state = TuiState::new(
        session_path.display().to_string(),
        tape.replay_turns().into_iter().map(turn_from_record).collect(),
        prompt_seed,
        remembered_selection,
        startup_notice,
    );
    let mut runtime = None;
    let mut subscriber = None;
    let mut tape_slot = Some(tape);

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    let guard = TerminalRestoreGuard;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let loop_result = run_tui_loop_inner(
        &mut terminal,
        &mut state,
        &mut registry,
        store_path,
        &mut tape_slot,
        &mut runtime,
        &mut subscriber,
        session_path,
    );

    terminal.show_cursor()?;
    drop(terminal);
    drop(guard);

    if let Err(error) = loop_result {
        return Err(error);
    }

    if let Some(runtime) = runtime.as_mut() {
        let handoff = finalize_runtime(runtime, session_path)?;
        println!("交接摘要：{}", handoff.summary);
        println!("下一步：");
        for step in handoff.next_steps {
            println!("- {step}");
        }
    }

    Ok(())
}

fn run_tui_loop_inner(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut TuiState,
    registry: &mut ProviderRegistry,
    store_path: &Path,
    tape_slot: &mut Option<SessionTape>,
    runtime: &mut Option<CliRuntime>,
    subscriber: &mut Option<RuntimeSubscriberId>,
    session_path: &Path,
) -> Result<(), CliLoopError> {
    loop {
        terminal.draw(|frame| draw_tui(frame, state, registry))?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
            KeyCode::Esc => break,
            KeyCode::Tab => {
                state.focus = state.focus.next_for_phase(&state.phase);
                state.status = Some(format!("当前焦点：{}", state.focus.label()));
                continue;
            }
            KeyCode::Up if matches!(state.phase, Phase::Chat) => {
                state.scroll_up();
                continue;
            }
            KeyCode::Down if matches!(state.phase, Phase::Chat) => {
                state.scroll_down();
                continue;
            }
            _ => {}
        }

        match state.phase.clone() {
            Phase::SelectProvider => {
                handle_select_provider_key(
                    key.code,
                    state,
                    registry,
                    store_path,
                    tape_slot,
                    session_path,
                )?;
            }
            Phase::CreateProvider { step, draft } => {
                handle_create_provider_key(
                    key.code,
                    state,
                    registry,
                    store_path,
                    tape_slot,
                    session_path,
                    step,
                    draft,
                )?;
            }
            Phase::InitialPrompt { selection } => {
                if let Some((model_label, built_runtime, built_subscriber)) =
                    handle_initial_prompt_key(
                        key.code,
                        state,
                        tape_slot,
                        session_path,
                        selection.clone(),
                    )?
                {
                    state.model_label = model_label;
                    *subscriber = Some(built_subscriber);
                    *runtime = Some(built_runtime);
                    state.phase = Phase::Chat;
                }
            }
            Phase::Chat => {
                let Some(runtime) = runtime.as_mut() else {
                    state.status = Some("运行时尚未就绪。".into());
                    continue;
                };
                let Some(subscriber) = *subscriber else {
                    state.status = Some("订阅者尚未就绪。".into());
                    continue;
                };
                handle_chat_key(key.code, state, runtime, subscriber, session_path)?;
                if state.should_exit {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn handle_select_provider_key(
    key: KeyCode,
    state: &mut TuiState,
    registry: &mut ProviderRegistry,
    store_path: &Path,
    tape_slot: &mut Option<SessionTape>,
    session_path: &Path,
) -> Result<(), CliLoopError> {
    let options = startup_options(registry);
    match key {
        KeyCode::Up => {
            if state.selected_option > 0 {
                state.selected_option -= 1;
            }
        }
        KeyCode::Down => {
            if state.selected_option + 1 < options.len() {
                state.selected_option += 1;
            }
        }
        KeyCode::Enter => match options[state.selected_option].clone() {
            StartupOption::Existing(profile) => {
                registry.set_active(&profile.name).map_err(io_error)?;
                registry.save(store_path).map_err(io_error)?;
                persist_provider_binding(
                    tape_slot,
                    session_path,
                    SessionProviderBinding::Provider {
                        name: profile.name.clone(),
                        model: profile.model.clone(),
                        base_url: profile.base_url.clone(),
                    },
                )?;
                state.input = state.prompt_seed.clone().unwrap_or_default();
                state.model_label = format!("openai/{}", profile.model);
                state.phase =
                    Phase::InitialPrompt { selection: ProviderLaunchChoice::OpenAi(profile) };
                state.status =
                    Some("当前会话已沿用该 provider，请输入首条问题；按 F2 可替换。".into());
            }
            StartupOption::CreateOpenAi => {
                state.input.clear();
                state.phase = Phase::CreateProvider {
                    step: CreateProviderStep::Name,
                    draft: ProviderDraft::default(),
                };
                state.status = Some("请输入 provider 名称。".into());
            }
            StartupOption::Bootstrap => {
                persist_provider_binding(
                    tape_slot,
                    session_path,
                    SessionProviderBinding::Bootstrap,
                )?;
                state.input = state.prompt_seed.clone().unwrap_or_default();
                state.model_label = "local/bootstrap".into();
                state.phase = Phase::InitialPrompt { selection: ProviderLaunchChoice::Bootstrap };
                state.status = Some("当前会话将使用本地 bootstrap；按 F2 可替换。".into());
            }
        },
        _ => {}
    }
    Ok(())
}

fn handle_create_provider_key(
    key: KeyCode,
    state: &mut TuiState,
    registry: &mut ProviderRegistry,
    store_path: &Path,
    tape_slot: &mut Option<SessionTape>,
    session_path: &Path,
    mut step: CreateProviderStep,
    mut draft: ProviderDraft,
) -> Result<(), CliLoopError> {
    match key {
        KeyCode::Backspace => {
            state.input.pop();
        }
        KeyCode::Char(ch) => {
            state.input.push(ch);
        }
        KeyCode::Enter => {
            let value = state.input.trim().to_string();
            match step {
                CreateProviderStep::Name => {
                    if value.is_empty() {
                        state.status = Some("provider 名称不能为空。".into());
                    } else {
                        draft.name = value;
                        state.input.clear();
                        step = CreateProviderStep::Model;
                        state.status = Some("请输入模型名称。".into());
                        state.phase = Phase::CreateProvider { step, draft };
                    }
                }
                CreateProviderStep::Model => {
                    if value.is_empty() {
                        state.status = Some("模型名称不能为空。".into());
                    } else {
                        draft.model = value;
                        state.input.clear();
                        step = CreateProviderStep::ApiKey;
                        state.status = Some("请输入 API Key。".into());
                        state.phase = Phase::CreateProvider { step, draft };
                    }
                }
                CreateProviderStep::ApiKey => {
                    if value.is_empty() {
                        state.status = Some("API Key 不能为空。".into());
                    } else {
                        draft.api_key = value;
                        state.input.clear();
                        state.input = "https://api.openai.com/v1".into();
                        step = CreateProviderStep::BaseUrl;
                        state.status = Some("请输入 Base URL，直接回车使用默认值。".into());
                        state.phase = Phase::CreateProvider { step, draft };
                    }
                }
                CreateProviderStep::BaseUrl => {
                    draft.base_url =
                        if value.is_empty() { "https://api.openai.com/v1".into() } else { value };
                    let profile = ProviderProfile::openai_responses(
                        draft.name.clone(),
                        draft.base_url.clone(),
                        draft.api_key.clone(),
                        draft.model.clone(),
                    );
                    registry.upsert(profile.clone());
                    registry.set_active(&profile.name).map_err(io_error)?;
                    registry.save(store_path).map_err(io_error)?;
                    persist_provider_binding(
                        tape_slot,
                        session_path,
                        SessionProviderBinding::Provider {
                            name: profile.name.clone(),
                            model: profile.model.clone(),
                            base_url: profile.base_url.clone(),
                        },
                    )?;
                    state.input = state.prompt_seed.clone().unwrap_or_default();
                    state.model_label = format!("openai/{}", profile.model);
                    state.phase =
                        Phase::InitialPrompt { selection: ProviderLaunchChoice::OpenAi(profile) };
                    state.status = Some("provider 已创建并绑定到当前会话；按 F2 可替换。".into());
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_initial_prompt_key(
    key: KeyCode,
    state: &mut TuiState,
    tape_slot: &mut Option<SessionTape>,
    session_path: &Path,
    selection: ProviderLaunchChoice,
) -> Result<Option<(String, CliRuntime, RuntimeSubscriberId)>, CliLoopError> {
    match key {
        KeyCode::F(2) => {
            state.phase = Phase::SelectProvider;
            state.selected_option = 0;
            state.status = Some("请重新选择 provider。".into());
        }
        KeyCode::Backspace => {
            state.input.pop();
        }
        KeyCode::Char(ch) => {
            state.input.push(ch);
        }
        KeyCode::Enter => {
            let prompt = state.input.trim().to_string();
            if prompt.is_empty() {
                state.status = Some("请输入首条问题。".into());
                return Ok(None);
            }

            let (identity, model) = build_model_from_selection(selection)
                .map_err(|error| CliLoopError::Io(io::Error::other(error.to_string())))?;
            let model_label = format!("{}/{}", identity.provider, identity.name);
            let mut runtime = AgentRuntime::with_tape(
                model,
                BootstrapTools,
                identity,
                tape_slot.take().unwrap_or_default(),
            )
            .with_instructions("你是 like 的起步代理。优先给出结构化、可继续落地的答案。");
            runtime.disable_tool("handoff_session");
            let subscriber = runtime.subscribe();
            let (events, visible_tool_count, error) = super::loop_driver::try_process_turn(
                &mut runtime,
                subscriber,
                prompt,
                session_path,
            );
            for event in events {
                if let RuntimeEvent::TurnLifecycle { turn } = event {
                    state.current_turns.push(turn);
                }
            }
            state.visible_tool_count = visible_tool_count;
            state.input.clear();
            if let Some(error) = error {
                state.status = Some(error.to_string());
                return Err(error);
            }
            state.status = Some("轮次已保存到会话索引。".into());
            return Ok(Some((model_label, runtime, subscriber)));
        }
        _ => {}
    }
    Ok(None)
}

fn handle_chat_key(
    key: KeyCode,
    state: &mut TuiState,
    runtime: &mut CliRuntime,
    subscriber: RuntimeSubscriberId,
    session_path: &Path,
) -> Result<(), CliLoopError> {
    match key {
        KeyCode::F(2) => {
            state.status = Some("当前最小版本仅支持在会话开始前替换 provider。".into());
            return Ok(());
        }
        KeyCode::Backspace => {
            state.input.pop();
        }
        KeyCode::Char(ch) => {
            state.input.push(ch);
        }
        KeyCode::Enter => {
            let prompt = state.input.trim().to_string();
            state.input.clear();
            if prompt.is_empty() {
                state.status = Some("请输入非空内容，或输入 退出 结束。".into());
                return Ok(());
            }
            if is_exit_command(&prompt) {
                state.status = Some("已退出 like agent loop".into());
                state.should_exit = true;
                return Ok(());
            }

            let (events, visible_tool_count, error) =
                try_process_turn(runtime, subscriber, prompt, session_path);
            for event in events {
                if let RuntimeEvent::TurnLifecycle { turn } = event {
                    state.current_turns.push(turn);
                }
            }
            state.visible_tool_count = visible_tool_count;
            if let Some(error) = error {
                state.status = Some(error.to_string());
                return Err(error);
            }
            state.status = Some("轮次已保存到会话索引。".into());
        }
        _ => {}
    }
    Ok(())
}

#[derive(Clone)]
struct TuiState {
    model_label: String,
    session_label: String,
    replay_turns: Vec<TurnLifecycle>,
    current_turns: Vec<TurnLifecycle>,
    input: String,
    status: Option<String>,
    visible_tool_count: Option<usize>,
    phase: Phase,
    selected_option: usize,
    prompt_seed: Option<String>,
    should_exit: bool,
    focus: FocusArea,
    replay_scroll: usize,
    current_scroll: usize,
}

impl TuiState {
    fn new(
        session_label: String,
        turns: Vec<TurnLifecycle>,
        prompt_seed: Option<String>,
        remembered_selection: Option<ProviderLaunchChoice>,
        startup_notice: Option<String>,
    ) -> Self {
        let (model_label, phase, input, status) = match remembered_selection {
            Some(ProviderLaunchChoice::Bootstrap) => (
                "local/bootstrap".into(),
                Phase::InitialPrompt { selection: ProviderLaunchChoice::Bootstrap },
                prompt_seed.clone().unwrap_or_default(),
                startup_notice
                    .or(Some("已记住上次使用的 bootstrap；按 F2 可替换 provider。".into())),
            ),
            Some(ProviderLaunchChoice::OpenAi(profile)) => (
                format!("openai/{}", profile.model),
                Phase::InitialPrompt { selection: ProviderLaunchChoice::OpenAi(profile) },
                prompt_seed.clone().unwrap_or_default(),
                startup_notice.or(Some("已记住当前会话上次使用的 provider；按 F2 可替换。".into())),
            ),
            None => (
                "未选择 provider".into(),
                Phase::SelectProvider,
                String::new(),
                startup_notice.or(Some("请选择 provider，然后输入首条问题。".into())),
            ),
        };

        Self {
            model_label,
            session_label,
            replay_turns: turns,
            current_turns: Vec::new(),
            input,
            status,
            visible_tool_count: None,
            phase,
            selected_option: 0,
            prompt_seed,
            should_exit: false,
            focus: FocusArea::Input,
            replay_scroll: 0,
            current_scroll: 0,
        }
    }

    fn scroll_up(&mut self) {
        match self.focus {
            FocusArea::Replay => self.replay_scroll = self.replay_scroll.saturating_sub(1),
            FocusArea::Current => self.current_scroll = self.current_scroll.saturating_sub(1),
            FocusArea::Input => {}
        }
    }

    fn scroll_down(&mut self) {
        match self.focus {
            FocusArea::Replay => self.replay_scroll = self.replay_scroll.saturating_add(1),
            FocusArea::Current => self.current_scroll = self.current_scroll.saturating_add(1),
            FocusArea::Input => {}
        }
    }
}

#[derive(Clone, Copy)]
enum FocusArea {
    Input,
    Replay,
    Current,
}

impl FocusArea {
    fn next(self) -> Self {
        match self {
            Self::Input => Self::Replay,
            Self::Replay => Self::Current,
            Self::Current => Self::Input,
        }
    }

    fn next_for_phase(self, phase: &Phase) -> Self {
        match phase {
            Phase::Chat => self.next(),
            _ => match self {
                Self::Input => Self::Replay,
                Self::Replay => Self::Input,
                Self::Current => Self::Input,
            },
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Input => "输入",
            Self::Replay => "回放面板",
            Self::Current => "本次运行",
        }
    }
}

#[derive(Clone)]
enum Phase {
    SelectProvider,
    CreateProvider { step: CreateProviderStep, draft: ProviderDraft },
    InitialPrompt { selection: ProviderLaunchChoice },
    Chat,
}

#[derive(Clone)]
enum CreateProviderStep {
    Name,
    Model,
    ApiKey,
    BaseUrl,
}

#[derive(Clone, Default)]
struct ProviderDraft {
    name: String,
    model: String,
    api_key: String,
    base_url: String,
}

#[derive(Clone)]
enum StartupOption {
    Existing(ProviderProfile),
    CreateOpenAi,
    Bootstrap,
}

fn startup_options(registry: &ProviderRegistry) -> Vec<StartupOption> {
    let mut options =
        registry.providers().iter().cloned().map(StartupOption::Existing).collect::<Vec<_>>();
    options.push(StartupOption::CreateOpenAi);
    options.push(StartupOption::Bootstrap);
    options
}

fn selection_from_binding(
    binding: &SessionProviderBinding,
    registry: &ProviderRegistry,
) -> Option<ProviderLaunchChoice> {
    match binding {
        SessionProviderBinding::Bootstrap => Some(ProviderLaunchChoice::Bootstrap),
        SessionProviderBinding::Provider { name, model, base_url } => registry
            .providers()
            .iter()
            .find(|provider| {
                provider.name == *name && provider.model == *model && provider.base_url == *base_url
            })
            .cloned()
            .map(ProviderLaunchChoice::OpenAi),
    }
}

fn resolve_remembered_selection(
    tape: &SessionTape,
    registry: &ProviderRegistry,
) -> (Option<ProviderLaunchChoice>, Option<String>) {
    let Some(binding) = tape.latest_provider_binding() else {
        return (None, None);
    };

    match selection_from_binding(&binding, registry) {
        Some(selection) => (Some(selection), None),
        None => (None, Some("当前会话记住的 provider 已缺失或配置已变化，请重新选择。".into())),
    }
}

fn persist_provider_binding(
    tape_slot: &mut Option<SessionTape>,
    session_path: &Path,
    binding: SessionProviderBinding,
) -> Result<(), CliLoopError> {
    if let Some(tape) = tape_slot.as_mut() {
        tape.bind_provider(binding);
        tape.save_jsonl(session_path)?;
    }
    Ok(())
}

fn io_error(error: impl std::fmt::Display) -> CliLoopError {
    CliLoopError::Io(io::Error::other(error.to_string()))
}

fn turn_from_record(record: session_tape::TurnRecord) -> TurnLifecycle {
    TurnLifecycle {
        turn_id: record.turn_id,
        started_at_ms: record.started_at_ms,
        finished_at_ms: record.finished_at_ms,
        source_entry_ids: record.source_entry_ids,
        user_message: record.user_message,
        assistant_message: record.assistant_message,
        tool_invocations: record
            .tool_invocations
            .into_iter()
            .map(|invocation| agent_runtime::ToolInvocationLifecycle {
                call: invocation.call,
                outcome: match invocation.outcome {
                    session_tape::ToolInvocationRecordOutcome::Succeeded { result } => {
                        agent_runtime::ToolInvocationOutcome::Succeeded { result }
                    }
                    session_tape::ToolInvocationRecordOutcome::Failed { message } => {
                        agent_runtime::ToolInvocationOutcome::Failed { message }
                    }
                },
            })
            .collect(),
        failure_message: record.failure_message,
    }
}

fn draw_tui(frame: &mut ratatui::Frame<'_>, state: &TuiState, registry: &ProviderRegistry) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(3),
            Constraint::Length(4),
        ])
        .split(frame.area());

    let header = Paragraph::new(Text::from(vec![
        Line::from(format!("模型：{}", state.model_label)),
        Line::from(format!("会话：{}", state.session_label)),
        Line::from(format!("阶段：{}", phase_label(&state.phase))),
    ]))
    .block(Block::default().title("状态").borders(Borders::ALL));
    frame.render_widget(header, layout[0]);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(layout[1]);

    draw_replay_panel(frame, main[0], state);

    match &state.phase {
        Phase::SelectProvider => draw_provider_selection(frame, main[1], state, registry),
        Phase::CreateProvider { step, .. } => draw_provider_creation(frame, main[1], state, step),
        Phase::InitialPrompt { .. } => draw_prompt_capture(frame, main[1], state),
        Phase::Chat => draw_current_session(frame, main[1], state),
    }

    let meta = Paragraph::new(Text::from(vec![Line::from(format!(
        "可见工具数：{}",
        state.visible_tool_count.unwrap_or_default()
    ))]))
    .block(Block::default().title("轮次元数据").borders(Borders::ALL));
    frame.render_widget(meta, layout[2]);

    let status_text =
        state.status.clone().unwrap_or_else(|| "输入内容后回车提交，Esc / Ctrl-C 退出。".into());
    let input = Paragraph::new(Text::from(vec![
        Line::from(format!("输入：{}", state.input)),
        Line::from("快捷键：Tab切换焦点  ↑↓滚动  F2替换provider(启动阶段)")
            .style(Style::default().add_modifier(Modifier::DIM)),
        Line::from(status_text).style(Style::default().add_modifier(Modifier::DIM)),
    ]))
    .block(focused_block("输入", state.focus, FocusArea::Input));
    frame.render_widget(input, layout[3]);
}

fn draw_replay_panel(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    state: &TuiState,
) {
    let lines = state.replay_turns.iter().rev().flat_map(turn_lines).collect::<Vec<_>>();
    let panel = Paragraph::new(Text::from(lines))
        .block(focused_block("回放面板", state.focus, FocusArea::Replay))
        .wrap(Wrap { trim: false })
        .scroll((state.replay_scroll as u16, 0));
    frame.render_widget(panel, area);
}

fn draw_current_session(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    state: &TuiState,
) {
    let lines = state.current_turns.iter().rev().flat_map(turn_lines).collect::<Vec<_>>();
    let panel = Paragraph::new(Text::from(lines))
        .block(focused_block("本次运行", state.focus, FocusArea::Current))
        .wrap(Wrap { trim: false })
        .scroll((state.current_scroll as u16, 0));
    frame.render_widget(panel, area);
}

fn focused_block(title: &str, focus: FocusArea, target: FocusArea) -> Block<'static> {
    let title = if std::mem::discriminant(&focus) == std::mem::discriminant(&target) {
        format!("> {title}")
    } else {
        title.to_string()
    };
    Block::default().title(title).borders(Borders::ALL)
}

fn turn_lines(turn: &TurnLifecycle) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!(
        "[轮次] {} ({} -> {})",
        turn.turn_id, turn.started_at_ms, turn.finished_at_ms
    ))];
    lines.push(Line::from(format!("[用户] {}", turn.user_message)));
    if let Some(assistant) = &turn.assistant_message {
        lines.push(Line::from(format!("[助手] {assistant}")));
    }
    for invocation in &turn.tool_invocations {
        match &invocation.outcome {
            agent_runtime::ToolInvocationOutcome::Succeeded { result } => {
                lines.push(Line::from(format!(
                    "[工具调用] {} #{} -> {}",
                    invocation.call.tool_name, invocation.call.invocation_id, result.content
                )))
            }
            agent_runtime::ToolInvocationOutcome::Failed { message } => {
                lines.push(Line::from(format!(
                    "[工具调用失败] {} #{} -> {}",
                    invocation.call.tool_name, invocation.call.invocation_id, message
                )))
            }
        }
    }
    if let Some(failure) = &turn.failure_message {
        lines.push(Line::from(format!("[失败] {failure}")));
    }
    lines.push(Line::from(""));
    lines
}

fn draw_provider_selection(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    state: &TuiState,
    registry: &ProviderRegistry,
) {
    let active_name = registry.active_provider().map(|provider| provider.name.as_str());
    let items = startup_options(registry)
        .into_iter()
        .enumerate()
        .map(|(index, option)| {
            let prefix = if index == state.selected_option { "> " } else { "  " };
            let content = match option {
                StartupOption::Existing(profile) => {
                    let mark =
                        if active_name == Some(profile.name.as_str()) { " *当前" } else { "" };
                    format!("{prefix}使用 provider: {} ({}){mark}", profile.name, profile.model)
                }
                StartupOption::CreateOpenAi => {
                    format!("{prefix}创建新的 OpenAI Responses provider")
                }
                StartupOption::Bootstrap => format!("{prefix}使用本地 bootstrap"),
            };
            ListItem::new(content)
        })
        .collect::<Vec<_>>();

    let list =
        List::new(items).block(Block::default().title("选择 provider").borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn draw_provider_creation(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    state: &TuiState,
    step: &CreateProviderStep,
) {
    let prompt = match step {
        CreateProviderStep::Name => "请输入 provider 名称",
        CreateProviderStep::Model => "请输入模型名称",
        CreateProviderStep::ApiKey => "请输入 API Key",
        CreateProviderStep::BaseUrl => "请输入 Base URL，回车可用默认值",
    };
    let content = Paragraph::new(Text::from(vec![
        Line::from(prompt),
        Line::from("按 Enter 提交，Esc 退出整个程序。"),
    ]))
    .block(Block::default().title("创建 provider").borders(Borders::ALL))
    .wrap(Wrap { trim: false });
    frame.render_widget(content, area);
    let _ = state;
}

fn draw_prompt_capture(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    _state: &TuiState,
) {
    let content = Paragraph::new(Text::from(vec![
        Line::from("请输入首条问题，按 Enter 进入会话。"),
        Line::from("这个问题会成为进入 loop 之前的第一轮。"),
    ]))
    .block(Block::default().title("首条问题").borders(Borders::ALL))
    .wrap(Wrap { trim: false });
    frame.render_widget(content, area);
}

fn phase_label(phase: &Phase) -> &'static str {
    match phase {
        Phase::SelectProvider => "选择 provider",
        Phase::CreateProvider { .. } => "创建 provider",
        Phase::InitialPrompt { .. } => "首条问题",
        Phase::Chat => "会话中",
    }
}

struct TerminalRestoreGuard;

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use crate::model::ProviderLaunchChoice;

    use super::{
        CreateProviderStep, FocusArea, Phase, ProviderDraft, StartupOption, TuiState, draw_tui,
        resolve_remembered_selection, startup_options,
    };

    #[test]
    fn startup_options_会包含创建项与_bootstrap() {
        let registry = provider_registry::ProviderRegistry::default();
        let options = startup_options(&registry);

        assert!(matches!(options[0], StartupOption::CreateOpenAi));
        assert!(matches!(options[1], StartupOption::Bootstrap));
    }

    #[test]
    fn 已记住_provider_时启动直接进入首条问题阶段() {
        let state = TuiState::new(
            ".like/session.jsonl".into(),
            vec![],
            Some("第一句".into()),
            Some(ProviderLaunchChoice::Bootstrap),
            None,
        );

        assert!(matches!(state.phase, Phase::InitialPrompt { .. }));
        assert_eq!(state.model_label, "local/bootstrap");
        assert_eq!(state.input, "第一句");
    }

    #[test]
    fn 已记住的_provider_缺失时会要求重新选择() {
        let registry = provider_registry::ProviderRegistry::default();
        let mut tape = session_tape::SessionTape::new();
        tape.bind_provider(session_tape::SessionProviderBinding::Provider {
            name: "main".into(),
            model: "gpt-4.1-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
        });

        let (selection, notice) = resolve_remembered_selection(&tape, &registry);

        assert!(selection.is_none());
        assert!(notice.expect("应有提示").contains("请重新选择"));
    }

    #[test]
    fn tui_会渲染模型与轮次信息() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(".like/session.jsonl".into(), vec![], None, None, None);
        state.model_label = "local/bootstrap".into();
        state.phase = Phase::Chat;
        state.status = Some("状态正常".into());
        state.replay_turns.push(agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1, 2, 3],
            user_message: "你好".into(),
            assistant_message: Some("已收到：你好".into()),
            tool_invocations: vec![],
            failure_message: None,
        });

        let registry = provider_registry::ProviderRegistry::default();
        terminal.draw(|frame| draw_tui(frame, &state, &registry)).expect("绘制成功");
        let buffer = terminal.backend().buffer().clone();
        let text = buffer.content.iter().map(|cell| cell.symbol()).collect::<String>();

        assert!(text.contains("local/bootstrap"));
        assert!(text.contains("turn-1"));
    }

    #[test]
    fn 焦点切换会按顺序轮转() {
        let mut focus = FocusArea::Input;

        focus = focus.next_for_phase(&Phase::Chat);
        assert!(matches!(focus, FocusArea::Replay));
        focus = focus.next_for_phase(&Phase::Chat);
        assert!(matches!(focus, FocusArea::Current));
        focus = focus.next_for_phase(&Phase::Chat);
        assert!(matches!(focus, FocusArea::Input));
    }

    #[test]
    fn 非聊天阶段不会切到本次运行焦点() {
        let mut focus = FocusArea::Input;

        focus = focus.next_for_phase(&Phase::SelectProvider);
        assert!(matches!(focus, FocusArea::Replay));
        focus = focus.next_for_phase(&Phase::SelectProvider);
        assert!(matches!(focus, FocusArea::Input));
    }

    #[test]
    fn 创建_provider_阶段标签正确() {
        let phase = Phase::CreateProvider {
            step: CreateProviderStep::ApiKey,
            draft: ProviderDraft::default(),
        };

        assert_eq!(super::phase_label(&phase), "创建 provider");
    }
}
