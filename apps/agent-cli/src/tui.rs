use std::{io, path::Path, time::Duration};

use agent_runtime::{AgentRuntime, RuntimeEvent, RuntimeSubscriberId, TurnLifecycle};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use provider_registry::{ProviderProfile, ProviderRegistry};
use pulldown_cmark::{Event as MarkdownEvent, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};
use session_tape::{SessionProviderBinding, SessionTape};

use crate::{
    driver::{self, CliRuntime, DriverHandle},
    errors::CliLoopError,
    loop_driver::is_exit_command,
    model::{BootstrapTools, ProviderLaunchChoice, build_model_from_selection},
    theme,
};

pub fn run_tui_loop(
    mut registry: ProviderRegistry,
    store_path: &Path,
    tape: SessionTape,
    session_path: &Path,
    prompt_seed: Option<String>,
) -> Result<(), CliLoopError> {
    let (remembered_selection, startup_notice) = resolve_remembered_selection(&tape, &registry);
    let mut state = TuiState::new(
        tape.replay_turns().into_iter().map(turn_from_record).collect(),
        prompt_seed,
        remembered_selection,
        startup_notice,
    );
    let mut runtime = None;
    let mut driver = None;
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
        &mut driver,
        session_path,
    );

    terminal.show_cursor()?;
    drop(terminal);
    drop(guard);

    if let Err(error) = loop_result {
        return Err(error);
    }

    if let Some(mut driver) = driver {
        let handoff = driver::finalize_driver(&mut driver)?;
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
    driver: &mut Option<DriverHandle>,
    session_path: &Path,
) -> Result<(), CliLoopError> {
    loop {
        let had_new_turns = poll_driver_state(state, driver)?;
        if had_new_turns {
            auto_scroll(state);
        }
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
                state.focus = state.focus.next();
                continue;
            }
            KeyCode::Up => {
                if matches!(state.focus, FocusArea::Messages) {
                    state.scroll_up();
                    continue;
                }
            }
            KeyCode::Down => {
                if matches!(state.focus, FocusArea::Messages) {
                    state.scroll_down();
                    continue;
                }
            }
            KeyCode::PageUp => {
                if matches!(state.focus, FocusArea::Messages) {
                    state.page_up();
                    continue;
                }
            }
            KeyCode::PageDown => {
                if matches!(state.focus, FocusArea::Messages) {
                    state.page_down();
                    continue;
                }
            }
            KeyCode::Left => {
                state.cursor_left();
                continue;
            }
            KeyCode::Right => {
                state.cursor_right();
                continue;
            }
            KeyCode::Home | KeyCode::Char('a')
                if key.code == KeyCode::Home || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                if matches!(state.focus, FocusArea::Messages) && key.code == KeyCode::Home {
                    state.scroll_to_top();
                    continue;
                }
                state.cursor_pos = 0;
                continue;
            }
            KeyCode::End | KeyCode::Char('e')
                if key.code == KeyCode::End || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                if matches!(state.focus, FocusArea::Messages) && key.code == KeyCode::End {
                    state.scroll_to_bottom();
                    continue;
                }
                state.cursor_pos = state.input.chars().count();
                continue;
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.delete_word_back();
                continue;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.delete_to_start();
                continue;
            }
            KeyCode::Delete => {
                state.delete_at_cursor();
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
                    handle_initial_prompt_key(key.code, state, tape_slot, selection.clone())?
                {
                    state.model_label = model_label;
                    let mut handle = driver::spawn_driver(
                        built_runtime,
                        built_subscriber,
                        session_path.to_path_buf(),
                    );
                    submit_turn_to_driver(&mut handle, state)?;
                    *driver = Some(handle);
                    *runtime = None;
                    state.phase = Phase::Chat;
                }
            }
            Phase::Chat => {
                let Some(driver) = driver.as_mut() else {
                    state.status = Some("驱动尚未就绪。".into());
                    continue;
                };
                handle_chat_key(key.code, state, driver)?;
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
                state.set_input(state.prompt_seed.clone().unwrap_or_default());
                state.model_label = format!("openai/{}", profile.model);
                state.phase =
                    Phase::InitialPrompt { selection: ProviderLaunchChoice::OpenAi(profile) };
                state.status =
                    Some("当前会话已沿用该 provider，请输入首条问题；按 F2 可替换。".into());
            }
            StartupOption::CreateOpenAi => {
                state.clear_input();
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
                state.set_input(state.prompt_seed.clone().unwrap_or_default());
                state.model_label = "local/bootstrap".into();
                state.phase = Phase::InitialPrompt { selection: ProviderLaunchChoice::Bootstrap };
                state.status = Some("当前会话将使用本地 bootstrap；按 F2 可替换 provider。".into());
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
            state.backspace_at_cursor();
        }
        KeyCode::Char(ch) => {
            state.insert_char(ch);
        }
        KeyCode::Enter => {
            let value = state.input.trim().to_string();
            match step {
                CreateProviderStep::Name => {
                    if value.is_empty() {
                        state.status = Some("provider 名称不能为空。".into());
                    } else {
                        draft.name = value;
                        state.clear_input();
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
                        state.clear_input();
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
                        state.set_input("https://api.openai.com/v1".into());
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
                    state.set_input(state.prompt_seed.clone().unwrap_or_default());
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
    selection: ProviderLaunchChoice,
) -> Result<Option<(String, CliRuntime, RuntimeSubscriberId)>, CliLoopError> {
    match key {
        KeyCode::F(2) => {
            state.phase = Phase::SelectProvider;
            state.selected_option = 0;
            state.status = Some("请重新选择 provider。".into());
        }
        KeyCode::Backspace => {
            state.backspace_at_cursor();
        }
        KeyCode::Char(ch) => {
            state.insert_char(ch);
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
            .with_instructions("你是 aia 的起步代理。优先给出结构化、可继续落地的答案。");
            runtime.disable_tool("handoff_session");
            let subscriber = runtime.subscribe();
            state.pending_prompt = Some(prompt);
            state.clear_input();
            state.processing = true;
            state.status = Some("正在处理首轮输入...".into());
            return Ok(Some((model_label, runtime, subscriber)));
        }
        _ => {}
    }
    Ok(None)
}

fn handle_chat_key(
    key: KeyCode,
    state: &mut TuiState,
    driver: &mut DriverHandle,
) -> Result<(), CliLoopError> {
    if state.processing {
        state.status = Some("当前轮次仍在处理中，请稍候。".into());
        return Ok(());
    }

    match key {
        KeyCode::F(2) => {
            state.status = Some("当前最小版本仅支持在会话开始前替换 provider。".into());
            return Ok(());
        }
        KeyCode::Backspace => {
            state.backspace_at_cursor();
        }
        KeyCode::Char(ch) => {
            state.insert_char(ch);
        }
        KeyCode::Enter => {
            let prompt = state.input.trim().to_string();
            state.clear_input();
            if prompt.is_empty() {
                state.status = Some("请输入非空内容，或输入 退出 结束。".into());
                return Ok(());
            }
            if is_exit_command(&prompt) {
                state.status = Some("已退出 aia agent loop".into());
                state.should_exit = true;
                return Ok(());
            }
            state.pending_prompt = Some(prompt);
            submit_turn_to_driver(driver, state)?;
        }
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TuiState {
    model_label: String,
    replay_turns: Vec<TurnLifecycle>,
    current_turns: Vec<TurnLifecycle>,
    input: String,
    cursor_pos: usize,
    status: Option<String>,
    phase: Phase,
    selected_option: usize,
    prompt_seed: Option<String>,
    should_exit: bool,
    focus: FocusArea,
    message_scroll: usize,
    message_line_count: usize,
    message_viewport_height: usize,
    user_scrolled_up: bool,
    processing: bool,
    pending_prompt: Option<String>,
    spinner_tick: usize,
}

impl TuiState {
    fn new(
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

        let cursor_pos = input.chars().count();
        Self {
            model_label,
            replay_turns: turns,
            current_turns: Vec::new(),
            input,
            cursor_pos,
            status,
            phase,
            selected_option: 0,
            prompt_seed,
            should_exit: false,
            focus: FocusArea::Input,
            message_scroll: 0,
            message_line_count: 0,
            message_viewport_height: 1,
            user_scrolled_up: false,
            processing: false,
            pending_prompt: None,
            spinner_tick: 0,
        }
    }

    // -- Cursor-aware input helpers --

    fn insert_char(&mut self, ch: char) {
        let byte_idx = self.byte_offset_of_cursor();
        self.input.insert(byte_idx, ch);
        self.cursor_pos += 1;
    }

    fn backspace_at_cursor(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            let byte_idx = self.byte_offset_of_cursor();
            let ch = self.input[byte_idx..].chars().next().unwrap();
            self.input.replace_range(byte_idx..byte_idx + ch.len_utf8(), "");
        }
    }

    fn delete_at_cursor(&mut self) {
        let byte_idx = self.byte_offset_of_cursor();
        if byte_idx < self.input.len() {
            let ch = self.input[byte_idx..].chars().next().unwrap();
            self.input.replace_range(byte_idx..byte_idx + ch.len_utf8(), "");
        }
    }

    fn delete_word_back(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let chars: Vec<char> = self.input.chars().collect();
        let mut new_pos = self.cursor_pos;
        // skip trailing whitespace
        while new_pos > 0 && chars[new_pos - 1].is_whitespace() {
            new_pos -= 1;
        }
        // skip word chars
        while new_pos > 0 && !chars[new_pos - 1].is_whitespace() {
            new_pos -= 1;
        }
        let start_byte: usize = chars[..new_pos].iter().map(|c| c.len_utf8()).sum();
        let end_byte: usize = chars[..self.cursor_pos].iter().map(|c| c.len_utf8()).sum();
        self.input.replace_range(start_byte..end_byte, "");
        self.cursor_pos = new_pos;
    }

    fn delete_to_start(&mut self) {
        let byte_idx = self.byte_offset_of_cursor();
        self.input.replace_range(..byte_idx, "");
        self.cursor_pos = 0;
    }

    fn cursor_left(&mut self) {
        self.cursor_pos = self.cursor_pos.saturating_sub(1);
    }

    fn cursor_right(&mut self) {
        let max = self.input.chars().count();
        if self.cursor_pos < max {
            self.cursor_pos += 1;
        }
    }

    fn set_input(&mut self, s: String) {
        let len = s.chars().count();
        self.input = s;
        self.cursor_pos = len;
    }

    fn clear_input(&mut self) {
        self.input.clear();
        self.cursor_pos = 0;
    }

    fn byte_offset_of_cursor(&self) -> usize {
        self.input.chars().take(self.cursor_pos).map(|c| c.len_utf8()).sum()
    }

    // -- Scrolling --

    fn scroll_up(&mut self) {
        if matches!(self.focus, FocusArea::Messages) {
            self.message_scroll = self.message_scroll.saturating_sub(1);
            self.user_scrolled_up = true;
        }
    }

    fn scroll_down(&mut self) {
        if matches!(self.focus, FocusArea::Messages) {
            let max = self.max_message_scroll();
            if self.message_scroll < max {
                self.message_scroll += 1;
            }
            if self.message_scroll >= max {
                self.user_scrolled_up = false;
            }
        }
    }

    fn page_up(&mut self) {
        let step = self.message_viewport_height.max(1);
        self.message_scroll = self.message_scroll.saturating_sub(step);
        self.user_scrolled_up = self.message_scroll > 0;
    }

    fn page_down(&mut self) {
        let max = self.max_message_scroll();
        let step = self.message_viewport_height.max(1);
        self.message_scroll = (self.message_scroll + step).min(max);
        if self.message_scroll >= max {
            self.user_scrolled_up = false;
        }
    }

    fn scroll_to_top(&mut self) {
        self.message_scroll = 0;
        self.user_scrolled_up = true;
    }

    fn scroll_to_bottom(&mut self) {
        self.message_scroll = self.max_message_scroll();
        self.user_scrolled_up = false;
    }

    fn max_message_scroll(&self) -> usize {
        self.message_line_count.saturating_sub(self.message_viewport_height.max(1))
    }

    fn clamp_scroll(&mut self) {
        let max = self.max_message_scroll();
        if self.message_scroll > max {
            self.message_scroll = max;
        }
    }
}

/// Auto-scroll when new turns arrive
fn auto_scroll(state: &mut TuiState) {
    if !state.user_scrolled_up {
        state.message_scroll = state.max_message_scroll();
    }
}

#[derive(Clone, Copy)]
enum FocusArea {
    Messages,
    Input,
}

impl FocusArea {
    fn next(self) -> Self {
        match self {
            Self::Messages => Self::Input,
            Self::Input => Self::Messages,
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

fn submit_turn_to_driver(
    driver: &mut DriverHandle,
    state: &mut TuiState,
) -> Result<(), CliLoopError> {
    let Some(prompt) = state.pending_prompt.take() else {
        return Ok(());
    };
    driver::submit_turn(driver, prompt).map_err(CliLoopError::from)?;
    state.processing = true;
    state.status = Some("正在处理中...".into());
    Ok(())
}

/// Returns true if any new turns were received.
fn poll_driver_state(
    state: &mut TuiState,
    driver: &mut Option<DriverHandle>,
) -> Result<bool, CliLoopError> {
    let Some(driver) = driver.as_mut() else {
        return Ok(false);
    };

    let mut had_new_turns = false;
    loop {
        match driver::poll_driver(driver).map_err(CliLoopError::from)? {
            Some(driver::DriverTurnResult { events, turn_error, persist_error }) => {
                state.processing = false;
                for event in events {
                    if let RuntimeEvent::TurnLifecycle { turn } = event {
                        state.current_turns.push(turn);
                        had_new_turns = true;
                    }
                }
                state.status = match (turn_error, persist_error) {
                    (Some(turn_error), Some(persist_error)) => {
                        Some(format!("轮次失败：{turn_error}；会话保存失败：{persist_error}"))
                    }
                    (Some(turn_error), None) => Some(format!("轮次失败：{turn_error}")),
                    (None, Some(persist_error)) => Some(format!("会话保存失败：{persist_error}")),
                    (None, None) => Some("轮次已保存到会话索引。".into()),
                };
            }
            None => break,
        }
    }

    Ok(had_new_turns)
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

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn draw_tui(frame: &mut ratatui::Frame<'_>, state: &mut TuiState, registry: &ProviderRegistry) {
    // Advance spinner
    if state.processing {
        state.spinner_tick = state.spinner_tick.wrapping_add(1);
    }

    match &state.phase {
        Phase::Chat => {
            // Chat phase: 2 zones — messages (fill) | input bar (3 lines)
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(4), Constraint::Length(3)])
                .split(frame.area());

            draw_messages(frame, layout[0], state);
            draw_input_bar(frame, layout[1], state);
        }
        Phase::SelectProvider => {
            // 3 zones: messages (top) | phase UI (mid) | input bar (bottom)
            let has_messages = !state.replay_turns.is_empty();
            let constraints = if has_messages {
                vec![Constraint::Percentage(40), Constraint::Min(4), Constraint::Length(3)]
            } else {
                vec![Constraint::Length(0), Constraint::Min(4), Constraint::Length(3)]
            };
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(frame.area());

            if has_messages {
                draw_messages(frame, layout[0], state);
            }
            draw_provider_selection(frame, layout[1], state, registry);
            draw_input_bar(frame, layout[2], state);
        }
        Phase::CreateProvider { step, .. } => {
            let step = step.clone();
            let has_messages = !state.replay_turns.is_empty();
            let constraints = if has_messages {
                vec![Constraint::Percentage(40), Constraint::Min(4), Constraint::Length(3)]
            } else {
                vec![Constraint::Length(0), Constraint::Min(4), Constraint::Length(3)]
            };
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(frame.area());

            if has_messages {
                draw_messages(frame, layout[0], state);
            }
            draw_provider_creation(frame, layout[1], state, &step);
            draw_input_bar(frame, layout[2], state);
        }
        Phase::InitialPrompt { .. } => {
            // Same as Chat — just messages + input bar; status bar already shows the phase
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(4), Constraint::Length(3)])
                .split(frame.area());

            draw_messages(frame, layout[0], state);
            draw_input_bar(frame, layout[1], state);
        }
    }
}

fn draw_messages(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut TuiState) {
    // Build unified message flow: replay_turns ++ current_turns (chronological)
    let all_turns: Vec<&TurnLifecycle> =
        state.replay_turns.iter().chain(state.current_turns.iter()).collect();

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, turn) in all_turns.iter().enumerate() {
        lines.extend(turn_lines(turn));
        // Empty line between turns (not after the last one)
        if i + 1 < all_turns.len() {
            lines.push(Line::from(""));
        }
    }

    // Spinner line when processing
    if state.processing {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        let frame_char = theme::SPINNER_FRAMES[state.spinner_tick % theme::SPINNER_FRAMES.len()];
        lines.push(Line::from(Span::styled(
            format!("{frame_char} Thinking..."),
            theme::SPINNER_STYLE,
        )));
    }

    let line_count = lines.len();
    state.message_line_count = line_count;
    state.message_viewport_height = area.height.max(1) as usize;
    state.clamp_scroll();

    let panel = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((state.message_scroll as u16, 0));
    frame.render_widget(panel, area);
}

fn draw_input_bar(frame: &mut ratatui::Frame<'_>, area: Rect, state: &TuiState) {
    // Thin separator line at the top of input area
    if area.width > 2 {
        let sep_line = "─".repeat((area.width - 2) as usize);
        let sep = Paragraph::new(Line::from(Span::styled(
            format!("╶{sep_line}╴"),
            theme::SEPARATOR_STYLE,
        )));
        let sep_area = Rect { x: area.x, y: area.y, width: area.width, height: 1 };
        frame.render_widget(sep, sep_area);
    }

    // Status bar content
    let tool_count: usize =
        state.current_turns.iter().map(|t| t.tool_invocations.len()).sum::<usize>()
            + state.replay_turns.iter().map(|t| t.tool_invocations.len()).sum::<usize>();

    let status_text = if state.processing {
        let frame_char = theme::SPINNER_FRAMES[state.spinner_tick % theme::SPINNER_FRAMES.len()];
        let base = state.status.clone().unwrap_or_else(|| "正在处理中...".into());
        format!("{frame_char} {base}")
    } else {
        state.status.clone().unwrap_or_else(|| "就绪".into())
    };
    let status_style = if state.processing { theme::SPINNER_STYLE } else { theme::DIM_STYLE };

    // Build 3 lines: input | status bar | hints
    let status_bar = format!(
        " {} │ {} │ tools: {tool_count} │ {status_text}",
        state.model_label,
        phase_label(&state.phase),
    );

    let content_area = Rect {
        x: area.x,
        y: area.y + 1, // skip separator line
        width: area.width,
        height: area.height.saturating_sub(1),
    };

    let input_widget = Paragraph::new(Text::from(vec![
        Line::from(format!("❯ {}", state.input)),
        Line::from(Span::styled(status_bar, status_style)),
    ]));
    frame.render_widget(input_widget, content_area);

    // Place terminal cursor
    let prefix_width: u16 = 3; // "❯ "
    let cursor_display_offset: u16 = state
        .input
        .chars()
        .take(state.cursor_pos)
        .fold(0u16, |acc, c| acc + if is_wide_char(c) { 2 } else { 1 });
    let cursor_x = area.x + prefix_width + cursor_display_offset;
    let cursor_y = area.y + 1; // after separator
    if cursor_x < area.x + area.width {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

/// Simple heuristic for CJK wide characters.
fn is_wide_char(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x3000..=0x303F).contains(&cp)
        || (0xFF00..=0xFFEF).contains(&cp)
        || (0x2E80..=0x2FFF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE4F).contains(&cp)
        || (0x20000..=0x2A6DF).contains(&cp)
}

struct MarkdownRenderState {
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    style_stack: Vec<Style>,
    list_depth: usize,
    pending_prefix: Option<(String, Style)>,
    quote_depth: usize,
    in_code_block: bool,
}

impl MarkdownRenderState {
    fn new(base_style: Style) -> Self {
        Self {
            lines: Vec::new(),
            current: Vec::new(),
            style_stack: vec![base_style],
            list_depth: 0,
            pending_prefix: None,
            quote_depth: 0,
            in_code_block: false,
        }
    }

    fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, style: Style) {
        self.style_stack.push(style);
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            let _ = self.style_stack.pop();
        }
    }

    fn push_prefix_if_needed(&mut self) {
        if !self.current.is_empty() {
            return;
        }
        if self.quote_depth > 0 {
            self.current
                .push(Span::styled("│ ".repeat(self.quote_depth), theme::MARKDOWN_QUOTE_STYLE));
        }
        if let Some((prefix, style)) = self.pending_prefix.take() {
            self.current.push(Span::styled(prefix, style));
        }
    }

    fn push_text(&mut self, text: &str, style: Style) {
        for (index, segment) in text.split('\n').enumerate() {
            if !segment.is_empty() {
                self.push_prefix_if_needed();
                self.current.push(Span::styled(segment.to_string(), style));
            }
            if index + 1 < text.split('\n').count() {
                self.flush_line(!segment.is_empty());
            }
        }
    }

    fn flush_line(&mut self, allow_empty: bool) {
        if !self.current.is_empty() || allow_empty {
            self.lines.push(Line::from(std::mem::take(&mut self.current)));
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_line(false);
        self.lines
    }
}

fn heading_prefix(level: HeadingLevel) -> String {
    let count = match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    };
    format!("{} ", "#".repeat(count))
}

fn markdown_lines(content: &str, base_style: Style) -> Vec<Line<'static>> {
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(content, options);
    let mut state = MarkdownRenderState::new(base_style);

    for event in parser {
        match event {
            MarkdownEvent::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    state.flush_line(false);
                    state.pending_prefix =
                        Some((heading_prefix(level), theme::MARKDOWN_HEADING_STYLE));
                    state.push_style(theme::MARKDOWN_HEADING_STYLE);
                }
                Tag::List(_) => {
                    state.flush_line(false);
                    state.list_depth += 1;
                }
                Tag::Item => {
                    state.flush_line(false);
                    let indent = "  ".repeat(state.list_depth.saturating_sub(1));
                    state.pending_prefix =
                        Some((format!("{indent}• "), theme::MARKDOWN_BULLET_STYLE));
                }
                Tag::BlockQuote(_) => {
                    state.flush_line(false);
                    state.quote_depth += 1;
                }
                Tag::CodeBlock(_) => {
                    state.flush_line(false);
                    state.in_code_block = true;
                    state.push_style(theme::MARKDOWN_CODE_BLOCK_STYLE);
                }
                Tag::Emphasis => {
                    state.push_style(state.current_style().add_modifier(Modifier::ITALIC));
                }
                Tag::Strong => {
                    state.push_style(state.current_style().add_modifier(Modifier::BOLD));
                }
                Tag::Strikethrough => {
                    state.push_style(state.current_style().add_modifier(Modifier::CROSSED_OUT));
                }
                _ => {}
            },
            MarkdownEvent::End(tag) => match tag {
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item => {
                    state.flush_line(false);
                    if matches!(tag, TagEnd::Heading(_)) {
                        state.pop_style();
                    }
                }
                TagEnd::List(_) => {
                    state.flush_line(false);
                    state.list_depth = state.list_depth.saturating_sub(1);
                }
                TagEnd::BlockQuote(_) => {
                    state.flush_line(false);
                    state.quote_depth = state.quote_depth.saturating_sub(1);
                }
                TagEnd::CodeBlock => {
                    state.flush_line(false);
                    state.in_code_block = false;
                    state.pop_style();
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                    state.pop_style();
                }
                _ => {}
            },
            MarkdownEvent::Text(text) => {
                let style = state.current_style();
                state.push_text(text.as_ref(), style);
            }
            MarkdownEvent::Code(code) => {
                state.push_prefix_if_needed();
                state
                    .current
                    .push(Span::styled(code.to_string(), theme::MARKDOWN_INLINE_CODE_STYLE));
            }
            MarkdownEvent::SoftBreak | MarkdownEvent::HardBreak => {
                state.flush_line(true);
            }
            MarkdownEvent::Rule => {
                state.flush_line(false);
                state.lines.push(Line::from(Span::styled(
                    "────────────".to_string(),
                    theme::SEPARATOR_STYLE,
                )));
            }
            MarkdownEvent::TaskListMarker(done) => {
                state.push_prefix_if_needed();
                let marker = if done { "[x] " } else { "[ ] " };
                state.current.push(Span::styled(marker.to_string(), theme::MARKDOWN_BULLET_STYLE));
            }
            MarkdownEvent::Html(html) | MarkdownEvent::InlineHtml(html) => {
                let style = state.current_style();
                state.push_text(html.as_ref(), style);
            }
            MarkdownEvent::FootnoteReference(text) => {
                state.push_prefix_if_needed();
                state.current.push(Span::styled(format!("[{text}]"), state.current_style()));
            }
            MarkdownEvent::InlineMath(text) | MarkdownEvent::DisplayMath(text) => {
                state.push_prefix_if_needed();
                state
                    .current
                    .push(Span::styled(text.to_string(), theme::MARKDOWN_INLINE_CODE_STYLE));
            }
        }
    }

    let lines = state.finish();
    if lines.is_empty() {
        vec![Line::from(Span::styled(content.to_string(), base_style))]
    } else {
        lines
    }
}

fn turn_lines(turn: &TurnLifecycle) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled("You", theme::USER_LABEL_STYLE)));
    lines.extend(markdown_lines(&turn.user_message, Style::default()));

    if let Some(assistant) = &turn.assistant_message {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Assistant", theme::ASSISTANT_LABEL_STYLE)));
        lines.extend(markdown_lines(assistant, Style::default()));
    }

    for invocation in &turn.tool_invocations {
        lines.push(Line::from(""));
        let tool_name = &invocation.call.tool_name;
        let call_id = &invocation.call.invocation_id;
        match &invocation.outcome {
            agent_runtime::ToolInvocationOutcome::Succeeded { result } => {
                lines.push(Line::from(Span::styled(
                    format!("┄ {tool_name} #{call_id} ┄┄┄"),
                    theme::TOOL_STYLE,
                )));
                lines.extend(markdown_lines(&result.content, theme::DIM_STYLE));
            }
            agent_runtime::ToolInvocationOutcome::Failed { message } => {
                lines.push(Line::from(Span::styled(
                    format!("┄ {tool_name} #{call_id} ┄┄┄"),
                    theme::TOOL_FAIL_STYLE,
                )));
                lines
                    .push(Line::from(Span::styled(format!("[失败] {message}"), theme::FAIL_STYLE)));
            }
        }
    }

    if let Some(failure) = &turn.failure_message {
        lines.push(Line::from(Span::styled(format!("[失败] {failure}"), theme::FAIL_STYLE)));
    }

    lines
}

/// Build a thin section header line: `╶── title ──────────╴`
fn section_header(title: &str, width: u16) -> Line<'static> {
    let label = format!(" {title} ");
    // 2 chars for ╶─ prefix, label, fill with ─, end with ╴
    let prefix = "╶─";
    let suffix = "╴";
    let used = prefix.len() + label.len() + suffix.len();
    let fill_count = (width as usize).saturating_sub(used);
    let fill: String = "─".repeat(fill_count);
    Line::from(Span::styled(format!("{prefix}{label}{fill}{suffix}"), theme::SEPARATOR_STYLE))
}

fn draw_provider_selection(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    registry: &ProviderRegistry,
) {
    let active_name = registry.active_provider().map(|provider| provider.name.as_str());

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Thin separator title
    lines.push(section_header("选择 provider", area.width));
    lines.push(Line::from(""));

    for (index, option) in startup_options(registry).into_iter().enumerate() {
        let prefix = if index == state.selected_option { "> " } else { "  " };
        let content = match option {
            StartupOption::Existing(profile) => {
                let mark = if active_name == Some(profile.name.as_str()) { " *当前" } else { "" };
                format!("{prefix}使用 provider: {} ({}){mark}", profile.name, profile.model)
            }
            StartupOption::CreateOpenAi => {
                format!("{prefix}创建新的 OpenAI Responses provider")
            }
            StartupOption::Bootstrap => format!("{prefix}使用本地 bootstrap"),
        };
        lines.push(Line::from(content));
    }

    let widget = Paragraph::new(Text::from(lines));
    frame.render_widget(widget, area);
}

fn draw_provider_creation(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    _state: &TuiState,
    step: &CreateProviderStep,
) {
    let prompt = match step {
        CreateProviderStep::Name => "请输入 provider 名称",
        CreateProviderStep::Model => "请输入模型名称",
        CreateProviderStep::ApiKey => "请输入 API Key",
        CreateProviderStep::BaseUrl => "请输入 Base URL，回车可用默认值",
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(section_header("创建 provider", area.width));
    lines.push(Line::from(""));
    lines.push(Line::from(prompt));
    lines.push(Line::from(Span::styled("按 Enter 提交，Esc 退出整个程序。", theme::DIM_STYLE)));

    let widget = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
            vec![],
            Some("第一句".into()),
            Some(ProviderLaunchChoice::Bootstrap),
            None,
        );

        assert!(matches!(state.phase, Phase::InitialPrompt { .. }));
        assert_eq!(state.model_label, "local/bootstrap");
        assert_eq!(state.input, "第一句");
        assert_eq!(state.cursor_pos, 3); // 3 chars
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
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
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
        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");
        let buffer = terminal.backend().buffer().clone();
        let text = buffer.content.iter().map(|cell| cell.symbol()).collect::<String>();

        assert!(text.contains("local/bootstrap"));
        assert!(text.contains("You"));
    }

    #[test]
    fn 焦点切换在两个区域间轮转() {
        let mut focus = FocusArea::Input;

        focus = focus.next();
        assert!(matches!(focus, FocusArea::Messages));
        focus = focus.next();
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

    fn line_text(line: &ratatui::text::Line<'_>) -> String {
        line.spans.iter().map(|span| span.content.as_ref()).collect::<Vec<_>>().join("")
    }

    #[test]
    fn turn_lines_会渲染基础_markdown_结构() {
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "请总结下面内容".into(),
            assistant_message: Some(
                "# 标题\n\n- 第一项\n- 第二项\n\n`命令`\n\n```rust\nfn main() {}\n```".into(),
            ),
            tool_invocations: vec![],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn);
        let text = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("# 标题"));
        assert!(text.contains("• 第一项"));
        assert!(text.contains("• 第二项"));
        assert!(text.contains("fn main() {}"));
    }

    #[test]
    fn cursor_insert_and_backspace() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.clear_input();

        state.insert_char('a');
        state.insert_char('b');
        state.insert_char('c');
        assert_eq!(state.input, "abc");
        assert_eq!(state.cursor_pos, 3);

        state.cursor_left();
        state.insert_char('X');
        assert_eq!(state.input, "abXc");
        assert_eq!(state.cursor_pos, 3);

        state.backspace_at_cursor();
        assert_eq!(state.input, "abc");
        assert_eq!(state.cursor_pos, 2);
    }

    #[test]
    fn delete_at_cursor_removes_char_under_cursor() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.set_input("hello".into());
        state.cursor_pos = 2;
        state.delete_at_cursor();
        assert_eq!(state.input, "helo");
        assert_eq!(state.cursor_pos, 2);
    }

    #[test]
    fn delete_word_back_removes_one_word() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.set_input("hello world".into());
        assert_eq!(state.cursor_pos, 11);
        state.delete_word_back();
        assert_eq!(state.input, "hello ");
        assert_eq!(state.cursor_pos, 6);
    }

    #[test]
    fn delete_to_start_clears_before_cursor() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.set_input("abcdef".into());
        state.cursor_pos = 3;
        state.delete_to_start();
        assert_eq!(state.input, "def");
        assert_eq!(state.cursor_pos, 0);
    }

    #[test]
    fn scroll_bounds_are_respected() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.focus = FocusArea::Messages;
        state.message_line_count = 5;
        state.message_scroll = 10;
        state.clamp_scroll();
        assert_eq!(state.message_scroll, 4); // max = 5-1 = 4
    }

    #[test]
    fn 消息滚动上限会考虑视口高度() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.focus = FocusArea::Messages;
        state.message_line_count = 20;
        state.message_viewport_height = 5;
        state.message_scroll = 99;

        state.clamp_scroll();

        assert_eq!(state.message_scroll, 15);
    }

    #[test]
    fn spinner_tick_advances_during_processing() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.processing = true;
        assert_eq!(state.spinner_tick, 0);
        // Simulating what draw_tui does
        state.spinner_tick = state.spinner_tick.wrapping_add(1);
        assert_eq!(state.spinner_tick, 1);
    }

    #[test]
    fn user_scrolled_up_flag_works() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.focus = FocusArea::Messages;
        state.message_line_count = 10;

        // Scrolling up sets the flag
        state.scroll_up();
        assert!(state.user_scrolled_up);

        // Scrolling to bottom resets it
        state.message_scroll = 8;
        state.scroll_down();
        assert_eq!(state.message_scroll, 9);
        assert!(!state.user_scrolled_up);
    }

    #[test]
    fn auto_scroll_respects_user_scrolled_up() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.message_line_count = 20;
        state.message_scroll = 5;

        // When user hasn't scrolled up, auto_scroll goes to bottom
        state.user_scrolled_up = false;
        super::auto_scroll(&mut state);
        assert_eq!(state.message_scroll, 19);

        // When user has scrolled up, auto_scroll doesn't move
        state.message_scroll = 5;
        state.user_scrolled_up = true;
        super::auto_scroll(&mut state);
        assert_eq!(state.message_scroll, 5);
    }
}
