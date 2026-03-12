use std::{collections::BTreeMap, io, path::Path, time::Duration};

use agent_core::StreamEvent;
use agent_runtime::{
    AgentRuntime, RuntimeEvent, RuntimeSubscriberId, ToolInvocationLifecycle,
    ToolInvocationOutcome, TurnLifecycle,
};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseEvent, MouseEventKind,
    },
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
    driver::{self, CliRuntime, DriverHandle, DriverPollResult},
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
    let mut state =
        TuiState::new(reconstruct_turns(&tape), prompt_seed, remembered_selection, startup_notice);
    let mut runtime = None;
    let mut driver = None;
    let mut tape_slot = Some(tape);

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
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
        driver::finalize_driver(&mut driver)?;
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
        if had_new_turns && !state.user_scrolled_up {
            state.pending_auto_scroll = true;
        }
        // Advance spinner each frame during streaming
        if state.streaming_turn.is_some() {
            state.spinner_tick = state.spinner_tick.wrapping_add(1);
        }
        terminal.draw(|frame| draw_tui(frame, state, registry))?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let input_event = event::read()?;
        if let Event::Mouse(mouse) = input_event {
            if handle_mouse_event(mouse, state) {
                continue;
            }
            continue;
        }
        let Event::Key(key) = input_event else {
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
            if let Some(cmd) = prompt.strip_prefix('/') {
                state.clear_input();
                handle_slash_command(cmd, state)?;
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
            if let Some(cmd) = prompt.strip_prefix('/') {
                return handle_slash_command(cmd, state);
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

fn handle_slash_command(cmd: &str, state: &mut TuiState) -> Result<(), CliLoopError> {
    match cmd.trim() {
        "logs" => {
            state.show_logs = !state.show_logs;
            state.status = Some(if state.show_logs {
                "日志面板已开启".into()
            } else {
                "日志面板已关闭".into()
            });
        }
        other => {
            state.status = Some(format!("未知命令: /{other}"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct StreamingTurn {
    user_message: String,
    status_text: Option<String>,
    thinking: String,
    text: String,
}

struct MessageView {
    lines: Vec<Line<'static>>,
    footer: Option<Line<'static>>,
}

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
    streaming_turn: Option<StreamingTurn>,
    log_lines: Vec<String>,
    show_logs: bool,
    message_area: Rect,
    pending_auto_scroll: bool,
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
            streaming_turn: None,
            log_lines: Vec::new(),
            show_logs: false,
            message_area: Rect::default(),
            pending_auto_scroll: false,
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
        if self.message_scroll > 0 {
            self.message_scroll = self.message_scroll.saturating_sub(1);
            self.user_scrolled_up = true;
        }
    }

    fn scroll_down(&mut self) {
        let max = self.max_message_scroll();
        if self.message_scroll < max {
            self.message_scroll += 1;
        }
        if self.message_scroll >= max {
            self.user_scrolled_up = false;
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
fn handle_mouse_event(mouse: MouseEvent, state: &mut TuiState) -> bool {
    let inside_messages = mouse.column >= state.message_area.x
        && mouse.column < state.message_area.x + state.message_area.width
        && mouse.row >= state.message_area.y
        && mouse.row < state.message_area.y + state.message_area.height;

    if !inside_messages {
        return false;
    }

    match mouse.kind {
        MouseEventKind::ScrollDown => {
            state.scroll_down();
            true
        }
        MouseEventKind::ScrollUp => {
            state.scroll_up();
            true
        }
        _ => false,
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
    state.streaming_turn = Some(StreamingTurn {
        user_message: prompt.clone(),
        status_text: Some("Thinking".into()),
        thinking: String::new(),
        text: String::new(),
    });
    driver::submit_turn(driver, prompt).map_err(CliLoopError::from)?;
    state.processing = true;
    state.status = Some("正在处理中...".into());
    state.user_scrolled_up = false;
    state.pending_auto_scroll = true;
    Ok(())
}

/// Returns true if any new content was received (deltas or completed turns).
fn poll_driver_state(
    state: &mut TuiState,
    driver: &mut Option<DriverHandle>,
) -> Result<bool, CliLoopError> {
    let Some(driver) = driver.as_mut() else {
        return Ok(false);
    };

    let mut had_updates = false;
    loop {
        match driver::poll_driver(driver).map_err(CliLoopError::from)? {
            DriverPollResult::StreamDelta(event) => match event {
                StreamEvent::ThinkingDelta { text } => {
                    let streaming = state.streaming_turn.get_or_insert_with(StreamingTurn::default);
                    streaming.status_text = Some("Thinking".into());
                    streaming.thinking.push_str(&text);
                    had_updates = true;
                    if !state.user_scrolled_up {
                        state.pending_auto_scroll = true;
                    }
                }
                StreamEvent::TextDelta { text } => {
                    let streaming = state.streaming_turn.get_or_insert_with(StreamingTurn::default);
                    streaming.status_text = Some("Responding".into());
                    streaming.text.push_str(&text);
                    had_updates = true;
                    if !state.user_scrolled_up {
                        state.pending_auto_scroll = true;
                    }
                }
                StreamEvent::Log { text } => {
                    state.log_lines.push(text);
                }
                StreamEvent::Done => {}
            },
            DriverPollResult::TurnCompleted(driver::DriverTurnResult {
                events,
                turn_error,
                persist_error,
            }) => {
                state.streaming_turn = None;
                state.processing = false;
                for event in events {
                    if let RuntimeEvent::TurnLifecycle { turn } = event {
                        state.current_turns.push(turn);
                        had_updates = true;
                    }
                }
                if !state.user_scrolled_up {
                    state.pending_auto_scroll = true;
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
            DriverPollResult::Nothing => break,
        }
    }

    Ok(had_updates)
}

fn reconstruct_turns(tape: &SessionTape) -> Vec<TurnLifecycle> {
    let mut groups: BTreeMap<String, Vec<&session_tape::TapeEntry>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    for entry in tape.entries() {
        if let Some(run_id) = entry.meta.get("run_id").and_then(|v| v.as_str()) {
            let key = run_id.to_string();
            if !groups.contains_key(&key) {
                order.push(key.clone());
            }
            groups.entry(key).or_default().push(entry);
        }
    }

    order
        .into_iter()
        .filter_map(|run_id| {
            let entries = groups.remove(&run_id)?;
            let user_message = entries
                .iter()
                .find_map(|e| e.as_message().filter(|m| m.role == agent_core::Role::User))
                .map(|m| m.content)?;
            let assistant_message = entries
                .iter()
                .find_map(|e| e.as_message().filter(|m| m.role == agent_core::Role::Assistant))
                .map(|m| m.content);

            let thinking = entries.iter().find_map(|e| e.as_thinking().map(|s| s.to_string()));

            let mut tool_invocations = Vec::new();
            let calls: Vec<_> =
                entries.iter().filter_map(|e| e.as_tool_call().map(|c| (e.id, c))).collect();
            for (call_id, call) in &calls {
                let outcome = entries
                    .iter()
                    .find_map(|e| {
                        let result = e.as_tool_result()?;
                        if result.invocation_id == call.invocation_id {
                            Some(ToolInvocationOutcome::Succeeded { result })
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| {
                        let fail_msg = entries
                            .iter()
                            .filter(|e| e.kind == "event")
                            .find_map(|e| {
                                let ids =
                                    e.meta.get("source_entry_ids").and_then(|v| v.as_array())?;
                                if ids.iter().any(|v| v.as_u64() == Some(*call_id)) {
                                    e.event_data()
                                        .and_then(|d| d.get("message"))
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_else(|| "unknown failure".into());
                        ToolInvocationOutcome::Failed { message: fail_msg }
                    });
                tool_invocations.push(ToolInvocationLifecycle { call: call.clone(), outcome });
            }

            let failure_message = entries.iter().find_map(|e| {
                if e.kind == "error" {
                    e.payload.get("message").and_then(|v| v.as_str()).map(|s| s.to_string())
                } else if e.kind == "event" && e.event_name() == Some("turn_failed") {
                    e.event_data()
                        .and_then(|d| d.get("message"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            });

            let source_entry_ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
            // Parse first/last date to approximate timestamps
            let started_at_ms = 0u128;
            let finished_at_ms = 0u128;

            Some(TurnLifecycle {
                turn_id: run_id,
                started_at_ms,
                finished_at_ms,
                source_entry_ids,
                user_message,
                assistant_message,
                thinking,
                tool_invocations,
                failure_message,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn draw_tui(frame: &mut ratatui::Frame<'_>, state: &mut TuiState, registry: &ProviderRegistry) {
    match &state.phase {
        Phase::Chat => {
            // Chat phase: 2 zones — messages (fill) | input bar (3 lines)
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(4), Constraint::Length(1), Constraint::Length(3)])
                .split(frame.area());

            if state.show_logs {
                let content = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                    .split(layout[0]);
                draw_messages(frame, content[0], state);
                draw_log_panel(frame, content[1], state);
            } else {
                draw_messages(frame, layout[0], state);
            }
            draw_input_bar(frame, layout[2], state);
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
                .constraints([Constraint::Min(4), Constraint::Length(1), Constraint::Length(3)])
                .split(frame.area());

            if state.show_logs {
                let content = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                    .split(layout[0]);
                draw_messages(frame, content[0], state);
                draw_log_panel(frame, content[1], state);
            } else {
                draw_messages(frame, layout[0], state);
            }
            draw_input_bar(frame, layout[2], state);
        }
    }
}

fn draw_messages(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut TuiState) {
    state.message_area = area;
    let view = message_lines(state, area.width);
    let has_footer = view.footer.is_some() && area.height > 0;
    let body_area = if has_footer {
        Rect { x: area.x, y: area.y, width: area.width, height: area.height.saturating_sub(1) }
    } else {
        area
    };

    let line_count = view.lines.len();
    state.message_line_count = line_count;
    state.message_viewport_height = body_area.height.max(1) as usize;
    if state.pending_auto_scroll && !state.user_scrolled_up {
        state.message_scroll = state.max_message_scroll();
        state.pending_auto_scroll = false;
    } else {
        state.clamp_scroll();
    }

    let panel = Paragraph::new(Text::from(view.lines))
        .wrap(Wrap { trim: false })
        .scroll((state.message_scroll as u16, 0));
    frame.render_widget(panel, body_area);

    if let Some(footer) = view.footer {
        let footer_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(footer), footer_area);
    }
}

fn message_lines(state: &TuiState, width: u16) -> MessageView {
    let all_turns: Vec<&TurnLifecycle> =
        state.replay_turns.iter().chain(state.current_turns.iter()).collect();

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (index, turn) in all_turns.iter().enumerate() {
        lines.extend(turn_lines(turn, width));
        if index + 1 < all_turns.len() {
            lines.push(Line::from(""));
        }
    }

    let mut footer = None;

    if let Some(ref streaming) = state.streaming_turn {
        if !lines.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(""));
        }
        if !streaming.user_message.is_empty() {
            lines.extend(user_message_lines(&streaming.user_message));
            lines.push(Line::from(""));
        }
        if !streaming.thinking.is_empty() {
            lines.extend(padded_plain_lines(inline_markdown_lines(
                "Thinking: ",
                &streaming.thinking,
                theme::thinking_label_style(),
                theme::thinking_style(),
            )));
            lines.push(Line::from(""));
        }
        if !streaming.text.is_empty() {
            lines.extend(padded_plain_lines(markdown_lines(
                &streaming.text,
                theme::assistant_message_style(),
            )));
        }
        if let Some(status_text) = &streaming.status_text {
            footer = Some(animated_status_line(status_text, state.spinner_tick));
        }
    } else if state.processing {
        footer = Some(animated_status_line("Thinking", state.spinner_tick));
    }

    MessageView { lines, footer }
}

fn patched_style(base: Style, overlay: Style) -> Style {
    base.patch(overlay)
}

fn draw_input_bar(frame: &mut ratatui::Frame<'_>, area: Rect, state: &TuiState) {
    // Thin separator line at the top of input area
    if area.width > 2 {
        let sep_line = "─".repeat((area.width - 2) as usize);
        let sep = Paragraph::new(Line::from(Span::styled(
            format!("╶{sep_line}╴"),
            theme::separator_style(),
        )));
        let sep_area = Rect { x: area.x, y: area.y, width: area.width, height: 1 };
        frame.render_widget(sep, sep_area);
    }

    let status_text = if state.processing {
        String::new()
    } else {
        state.status.clone().unwrap_or_else(|| "就绪".into())
    };
    let status_style = if state.processing { theme::spinner_style() } else { theme::dim_style() };

    // Build 3 lines: input | status bar | hints
    let status_bar = if state.processing {
        format!(" {} · {}", state.model_label, phase_label(&state.phase))
    } else {
        format!(" {} · {} · {status_text}", state.model_label, phase_label(&state.phase))
    };

    let content_area = Rect {
        x: area.x,
        y: area.y + 1, // skip separator line
        width: area.width,
        height: area.height.saturating_sub(1),
    };

    let input_widget = Paragraph::new(Text::from(vec![
        Line::from(format!("› {}", state.input)),
        Line::from(Span::styled(status_bar, status_style)),
    ]));
    frame.render_widget(input_widget, content_area);

    // Place terminal cursor
    let prefix_width: u16 = 2;
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
            self.current.push(Span::styled(
                "│ ".repeat(self.quote_depth),
                patched_style(self.current_style(), theme::markdown_quote_style()),
            ));
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
                    let heading_style =
                        patched_style(state.current_style(), theme::markdown_heading_style());
                    state.pending_prefix = Some((heading_prefix(level), heading_style));
                    state.push_style(heading_style);
                }
                Tag::List(_) => {
                    state.flush_line(false);
                    state.list_depth += 1;
                }
                Tag::Item => {
                    state.flush_line(false);
                    let indent = "  ".repeat(state.list_depth.saturating_sub(1));
                    state.pending_prefix = Some((
                        format!("{indent}• "),
                        patched_style(state.current_style(), theme::markdown_bullet_style()),
                    ));
                }
                Tag::BlockQuote(_) => {
                    state.flush_line(false);
                    state.quote_depth += 1;
                }
                Tag::CodeBlock(_) => {
                    state.flush_line(false);
                    state.in_code_block = true;
                    state.push_style(patched_style(
                        state.current_style(),
                        theme::markdown_code_block_style(),
                    ));
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
                state.current.push(Span::styled(
                    code.to_string(),
                    patched_style(state.current_style(), theme::markdown_inline_code_style()),
                ));
            }
            MarkdownEvent::SoftBreak | MarkdownEvent::HardBreak => {
                state.flush_line(true);
            }
            MarkdownEvent::Rule => {
                state.flush_line(false);
                state.lines.push(Line::from(Span::styled(
                    "────────────".to_string(),
                    patched_style(base_style, theme::separator_style()),
                )));
            }
            MarkdownEvent::TaskListMarker(done) => {
                state.push_prefix_if_needed();
                let marker = if done { "[x] " } else { "[ ] " };
                state.current.push(Span::styled(
                    marker.to_string(),
                    patched_style(state.current_style(), theme::markdown_bullet_style()),
                ));
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
                state.current.push(Span::styled(
                    text.to_string(),
                    patched_style(state.current_style(), theme::markdown_inline_code_style()),
                ));
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

fn prefixed_markdown_lines(
    content: &str,
    first_prefix: &str,
    rest_prefix: &str,
    style: Style,
) -> Vec<Line<'static>> {
    markdown_lines(content, style)
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let prefix = if index == 0 { first_prefix } else { rest_prefix };
            let mut spans = vec![Span::styled(prefix.to_string(), style)];
            spans.extend(line.spans.into_iter());
            Line::from(spans)
        })
        .collect()
}

fn tool_header_line(tool_name: &str, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled("• tool ", style),
        Span::styled(tool_name.to_string(), theme::tool_name_style()),
    ])
}

fn separator_line(width: u16) -> Line<'static> {
    Line::from(Span::styled("─".repeat(width.min(60) as usize), theme::separator_style()))
}

fn inline_thinking_lines(content: &str) -> Vec<Line<'static>> {
    inline_markdown_lines(
        "Thinking: ",
        content,
        theme::thinking_label_style(),
        theme::thinking_style(),
    )
}

fn inline_markdown_lines(
    label: &str,
    content: &str,
    label_style: Style,
    base_style: Style,
) -> Vec<Line<'static>> {
    let rendered_lines = markdown_lines(content, base_style);
    let mut lines = Vec::new();

    for (index, line) in rendered_lines.into_iter().enumerate() {
        let prefix = if index == 0 { label } else { "" };
        let mut spans = vec![Span::styled(prefix.to_string(), label_style)];
        spans.extend(line.spans.into_iter());
        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        vec![Line::from(Span::styled(label.to_string(), label_style))]
    } else {
        lines
    }
}

fn padded_plain_line(line: Line<'static>) -> Line<'static> {
    let mut spans = Vec::with_capacity(line.spans.len() + 2);
    spans.push(Span::raw(" "));
    spans.extend(line.spans);
    spans.push(Span::raw(" "));
    Line::from(spans)
}

fn padded_plain_lines(lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    lines.into_iter().map(padded_plain_line).collect()
}

fn padded_message_line(line: Line<'static>, style: Style) -> Line<'static> {
    let mut spans = Vec::with_capacity(line.spans.len() + 2);
    spans.push(Span::styled(" ".to_string(), style));
    spans.extend(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content.to_string(), patched_style(span.style, style))),
    );
    spans.push(Span::styled(" ".to_string(), style));
    Line::from(spans)
}

fn animated_status_line(text: &str, tick: usize) -> Line<'static> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Line::default();
    }
    let len = chars.len();
    let head = (tick / 2) % len;
    Line::from(
        chars
            .into_iter()
            .enumerate()
            .map(|(index, ch)| {
                let raw_distance = head.abs_diff(index);
                let distance = raw_distance.min(len.saturating_sub(raw_distance));
                let style = if index == head {
                    theme::status_head_style()
                } else if distance == 1 {
                    theme::status_trail_style()
                } else if distance == 2 {
                    theme::dim_style()
                } else {
                    theme::status_dim_style()
                };
                Span::styled(ch.to_string(), style)
            })
            .collect::<Vec<_>>(),
    )
}

fn user_message_lines(content: &str) -> Vec<Line<'static>> {
    markdown_lines(content, theme::user_message_style())
        .into_iter()
        .map(|line| padded_message_line(line, theme::user_message_style()))
        .collect()
}

fn turn_lines(turn: &TurnLifecycle, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.extend(user_message_lines(&turn.user_message));

    if let Some(thinking) = &turn.thinking {
        lines.push(Line::from(""));
        lines.extend(padded_plain_lines(inline_thinking_lines(thinking)));
    }

    for invocation in &turn.tool_invocations {
        lines.push(Line::from(""));
        let tool_name = &invocation.call.tool_name;
        match &invocation.outcome {
            agent_runtime::ToolInvocationOutcome::Succeeded { result } => {
                lines.push(padded_plain_line(tool_header_line(tool_name, theme::tool_style())));
                lines.extend(padded_plain_lines(prefixed_markdown_lines(
                    &result.content,
                    "  └ ",
                    "    ",
                    theme::dim_style(),
                )));
            }
            agent_runtime::ToolInvocationOutcome::Failed { message } => {
                lines
                    .push(padded_plain_line(tool_header_line(tool_name, theme::tool_fail_style())));
                lines.push(padded_plain_line(Line::from(Span::styled(
                    format!("  └ [失败] {message}"),
                    theme::fail_style(),
                ))));
            }
        }
    }

    if !turn.tool_invocations.is_empty() {
        if turn.assistant_message.is_some() {
            lines.push(Line::from(""));
            lines.push(separator_line(width));
        }
    }

    if let Some(assistant) = &turn.assistant_message {
        lines.push(Line::from(""));
        lines.extend(padded_plain_lines(markdown_lines(
            assistant,
            theme::assistant_message_style(),
        )));
    }

    if let Some(failure) = &turn.failure_message {
        lines.push(padded_plain_line(Line::from(Span::styled(
            format!("[失败] {failure}"),
            theme::fail_style(),
        ))));
    }

    lines
}

/// Build a thin section header line: `╶── title ──────────╴`
fn section_header(title: &str, width: u16) -> Line<'static> {
    section_header_styled(title, width, theme::separator_style())
}

fn section_header_styled(title: &str, width: u16, style: Style) -> Line<'static> {
    let label = format!(" {title} ");
    // 2 chars for ╶─ prefix, label, fill with ─, end with ╴
    let prefix = "╶─";
    let suffix = "╴";
    let used = prefix.len() + label.len() + suffix.len();
    let fill_count = (width as usize).saturating_sub(used);
    let fill: String = "─".repeat(fill_count);
    Line::from(Span::styled(format!("{prefix}{label}{fill}{suffix}"), style))
}

fn draw_log_panel(frame: &mut ratatui::Frame<'_>, area: Rect, state: &TuiState) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(section_header("Logs", area.width));
    if state.log_lines.is_empty() {
        lines.push(Line::from(Span::styled("(empty)", theme::dim_style())));
    } else {
        for entry in &state.log_lines {
            lines.push(Line::from(Span::styled(entry.clone(), theme::log_style())));
        }
    }
    // 自动滚动到底部
    let line_count = lines.len();
    let viewport = area.height.max(1) as usize;
    let scroll = line_count.saturating_sub(viewport);

    let panel =
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).scroll((scroll as u16, 0));
    frame.render_widget(panel, area);
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
    lines.push(Line::from(Span::styled("按 Enter 提交，Esc 退出整个程序。", theme::dim_style())));

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
        let _ = execute!(stdout, DisableMouseCapture, LeaveAlternateScreen);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    use ratatui::style::Color;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

    use agent_core::{ModelDisposition, ModelIdentity};
    use agent_runtime::AgentRuntime;

    use crate::{
        driver,
        model::{BootstrapModel, BootstrapTools, CliModel, ProviderLaunchChoice},
        theme,
    };

    use super::{
        CreateProviderStep, FocusArea, Phase, ProviderDraft, StartupOption, TuiState, draw_tui,
        resolve_remembered_selection, startup_options,
    };

    fn buffer_row_text(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>()
    }

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
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        });

        let registry = provider_registry::ProviderRegistry::default();
        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");
        let buffer = terminal.backend().buffer().clone();
        let text = buffer.content.iter().map(|cell| cell.symbol()).collect::<String>();
        let compact_text = text.replace(' ', "");

        assert!(text.contains("local/bootstrap"));
        assert!(compact_text.contains("你好"));
        assert!(!text.contains("You"));
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

    fn line_backgrounds(line: &ratatui::text::Line<'_>) -> Vec<Option<Color>> {
        line.spans.iter().map(|span| span.style.bg).collect::<Vec<_>>()
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
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 60);
        let text = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("# 标题"));
        assert!(text.contains("• 第一项"));
        assert!(text.contains("• 第二项"));
        assert!(text.contains("fn main() {}"));
    }

    #[test]
    fn turn_lines_会渲染_thinking_块() {
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "请分析".into(),
            assistant_message: Some("结论".into()),
            thinking: Some("先分析上下文，再给出答案".into()),
            tool_invocations: vec![],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 60);
        let text = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("Thinking"));
        assert!(text.contains("先分析上下文，再给出答案"));
        assert!(text.contains("结论"));
    }

    #[test]
    fn turn_lines_会把_thinking_首行内联且保留后续换行() {
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "请分析".into(),
            assistant_message: Some("正式回答".into()),
            thinking: Some("草拟 **推理**\n与 `代码`".into()),
            tool_invocations: vec![],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 60);
        let rendered = lines.iter().map(line_text).collect::<Vec<_>>();
        let thinking_index = rendered
            .iter()
            .position(|line| line.contains("Thinking: 草拟 推理"))
            .expect("应有 thinking 首行");

        assert_eq!(rendered[thinking_index + 1], "与 代码");
        assert!(!rendered[thinking_index].contains("**推理**"));
        assert!(!rendered[thinking_index + 1].contains("`代码`"));
        assert!(!rendered.iter().any(|line| line.contains("╶─ Thinking ")));
    }

    #[test]
    fn turn_lines_会给整块用户消息加背景而助手正文不加() {
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "用户消息\n第二行".into(),
            assistant_message: Some("助手消息".into()),
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 60);
        let user_line_one_bg = line_backgrounds(&lines[0]);
        let user_line_two_bg = line_backgrounds(&lines[1]);
        let assistant_line =
            lines.iter().find(|line| line_text(line).contains("助手消息")).expect("应有助手消息");
        let assistant_bgs = line_backgrounds(assistant_line);

        assert_eq!(line_text(&lines[0]), " 用户消息 ");
        assert_eq!(line_text(&lines[1]), " 第二行 ");
        assert!(user_line_one_bg.iter().all(|bg| *bg == theme::current().user_message_style.bg));
        assert!(user_line_two_bg.iter().all(|bg| *bg == theme::current().user_message_style.bg));
        assert!(assistant_bgs.iter().all(|bg| bg.is_none()));
    }

    #[test]
    fn turn_lines_不会显示_assistant_标题() {
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "你好".into(),
            assistant_message: Some("正式回答".into()),
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 60);
        let text = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("正式回答"));
        assert!(!text.contains("Assistant"));
    }

    #[test]
    fn tui_流式_thinking_会首行内联且保留换行() {
        let backend = TestBackend::new(100, 12);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.streaming_turn = Some(super::StreamingTurn {
            user_message: String::new(),
            status_text: None,
            thinking: "first **bold**\nnext `code`".into(),
            text: String::new(),
        });

        let registry = provider_registry::ProviderRegistry::default();
        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");
        let buffer = terminal.backend().buffer().clone();
        let row_one = buffer_row_text(&buffer, 0);
        let row_two = buffer_row_text(&buffer, 1);

        assert!(row_one.contains("Thinking: first bold"));
        assert!(row_two.starts_with("next code"));
        assert!(!row_one.contains("**bold**"));
        assert!(!row_two.contains("`code`"));
    }

    #[test]
    fn 提交后用户消息会立即出现在消息列表() {
        let identity = ModelIdentity::new("local", "bootstrap", ModelDisposition::Balanced);
        let mut runtime =
            AgentRuntime::new(CliModel::Bootstrap(BootstrapModel), BootstrapTools, identity)
                .with_instructions("保持简洁");
        let subscriber = runtime.subscribe();
        let session_path = std::env::temp_dir().join("aia-tui-submit-immediate.jsonl");
        let mut driver = driver::spawn_driver(runtime, subscriber, session_path);

        let mut state = TuiState::new(vec![], None, None, None);
        state.pending_prompt = Some("hello world".into());

        super::submit_turn_to_driver(&mut driver, &mut state).expect("提交成功");

        let streaming = state.streaming_turn.expect("应创建进行中轮次");
        assert_eq!(streaming.user_message, "hello world");
        assert_eq!(streaming.status_text.as_deref(), Some("Thinking"));
        assert!(state.processing);
        assert!(!state.user_scrolled_up);
        assert!(state.pending_auto_scroll);
    }

    #[test]
    fn 运行状态会显示在消息列表中() {
        let backend = TestBackend::new(100, 12);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.processing = true;
        state.status = Some("正在处理中...".into());
        state.streaming_turn = Some(super::StreamingTurn {
            user_message: "hello".into(),
            status_text: Some("Thinking".into()),
            thinking: String::new(),
            text: String::new(),
        });

        let registry = provider_registry::ProviderRegistry::default();
        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");
        let buffer = terminal.backend().buffer().clone();
        let text = buffer.content.iter().map(|cell| cell.symbol()).collect::<String>();

        assert!(text.contains("hello"));
        assert!(text.contains("Thinking"));
    }

    #[test]
    fn 状态动画会从左到右逐步变暗() {
        let line = super::animated_status_line("Thinking", 3);

        assert_eq!(line.spans[0].style, theme::status_trail_style());
        assert_eq!(line.spans[1].style, theme::status_head_style());
        assert_eq!(line.spans[2].style, theme::status_trail_style());
        assert_eq!(line.spans[4].style, theme::status_dim_style());
    }

    #[test]
    fn 鼠标滚轮可以滚动消息列表() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.message_area = Rect { x: 0, y: 0, width: 80, height: 10 };
        state.message_line_count = 40;
        state.message_viewport_height = 10;

        let consumed = super::handle_mouse_event(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 1,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
            &mut state,
        );

        assert!(consumed);
        assert!(state.message_scroll > 0);
    }

    #[test]
    fn 流式状态固定显示在消息列表最底部() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.processing = true;
        state.streaming_turn = Some(super::StreamingTurn {
            user_message: "hello".into(),
            status_text: Some("Thinking".into()),
            thinking: "first line".into(),
            text: "reply".into(),
        });

        let view = super::message_lines(&state, 60);
        let footer = view.footer.expect("应有底部状态行");

        assert!(line_text(&footer).contains("Thinking"));
    }

    #[test]
    fn draw_messages_会用最新视口信息自动滚动到底部() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.pending_auto_scroll = true;
        state.current_turns.push(agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "第一行\n第二行\n第三行\n第四行\n第五行".into(),
            assistant_message: Some("回复\n第二行\n第三行".into()),
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        });
        let registry = provider_registry::ProviderRegistry::default();

        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");

        assert_eq!(state.message_scroll, state.max_message_scroll());
        assert!(!state.pending_auto_scroll);
    }

    #[test]
    fn 流式轮次与历史消息之间保留两行空白() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.replay_turns.push(agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "历史消息".into(),
            assistant_message: Some("历史回复".into()),
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        });
        state.streaming_turn = Some(super::StreamingTurn {
            user_message: "当前输入".into(),
            status_text: Some("Thinking".into()),
            thinking: String::new(),
            text: String::new(),
        });

        let rendered = super::message_lines(&state, 60)
            .lines
            .into_iter()
            .map(|line| line_text(&line))
            .collect::<Vec<_>>();
        let current_index =
            rendered.iter().position(|line| line.contains(" 当前输入 ")).expect("应有当前输入");

        assert_eq!(rendered[current_index - 1], "");
        assert_eq!(rendered[current_index - 2], "");
    }

    #[test]
    fn 聊天布局会在消息区与输入框之间保留固定空白行() {
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        let registry = provider_registry::ProviderRegistry::default();

        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");

        let buffer = terminal.backend().buffer().clone();
        let gap_row = buffer_row_text(&buffer, 8);
        assert!(gap_row.trim().is_empty());
    }

    #[test]
    fn turn_lines_会把工具调用渲染为分层项目符号并置于回答前() {
        let call = agent_core::ToolCall::new("search_code").with_invocation_id("call-1");
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "请继续".into(),
            assistant_message: Some("正式回答".into()),
            thinking: None,
            tool_invocations: vec![agent_runtime::ToolInvocationLifecycle {
                call: call.clone(),
                outcome: agent_runtime::ToolInvocationOutcome::Succeeded {
                    result: agent_core::ToolResult::from_call(&call, "搜索结果\n第二行"),
                },
            }],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 60);
        let text = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(text.contains(" • tool search_code "));
        assert!(text.contains("   └ 搜索结果 "));
        assert!(text.contains("     第二行 "));
        assert!(text.contains("────────"));

        let tool_index = text.find("• tool search_code").expect("应有工具调用");
        let assistant_index = text.find("正式回答").expect("应有回答正文");
        assert!(tool_index < assistant_index);
    }

    #[test]
    fn turn_lines_会对工具成功输出应用_markdown_渲染() {
        let call = agent_core::ToolCall::new("search_code").with_invocation_id("call-1");
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "继续".into(),
            assistant_message: None,
            thinking: None,
            tool_invocations: vec![agent_runtime::ToolInvocationLifecycle {
                call: call.clone(),
                outcome: agent_runtime::ToolInvocationOutcome::Succeeded {
                    result: agent_core::ToolResult::from_call(&call, "**bold** and `cmd`"),
                },
            }],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 60);
        let text = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("bold"));
        assert!(text.contains("cmd"));
        assert!(!text.contains("**bold**"));
        assert!(!text.contains("`cmd`"));
    }

    #[test]
    fn turn_lines_会把失败工具调用渲染为层级结构() {
        let call = agent_core::ToolCall::new("search_code").with_invocation_id("call-1");
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "请继续".into(),
            assistant_message: None,
            thinking: None,
            tool_invocations: vec![agent_runtime::ToolInvocationLifecycle {
                call,
                outcome: agent_runtime::ToolInvocationOutcome::Failed {
                    message: "请求失败".into(),
                },
            }],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 60);
        let text = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(text.contains(" • tool search_code "));
        assert!(text.contains("   └ [失败] 请求失败 "));
    }

    #[test]
    fn tui_输入栏使用现代提示符与点分隔状态栏() {
        let backend = TestBackend::new(100, 12);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.model_label = "openai/gpt-4.1-mini".into();
        state.phase = Phase::Chat;
        state.status = Some("已连接".into());
        state.input = "hello".into();
        state.cursor_pos = 5;

        terminal
            .draw(|frame| super::draw_input_bar(frame, frame.area(), &state))
            .expect("绘制成功");
        let buffer = terminal.backend().buffer().clone();
        let input_row = buffer_row_text(&buffer, 1);
        let status_row = buffer_row_text(&buffer, 2);
        let compact_status = status_row.replace(' ', "");

        assert!(input_row.contains("› hello"));
        assert!(status_row.contains("openai/gpt-4.1-mini"));
        assert!(compact_status.contains("·会话中·已连接"));
        assert!(!status_row.contains("tools:"));
        assert!(!input_row.contains("❯ "));
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
        state.message_scroll = 3;

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
        state.message_viewport_height = 1;
        state.message_scroll = 5;

        state.user_scrolled_up = false;
        state.pending_auto_scroll = true;
        if state.pending_auto_scroll && !state.user_scrolled_up {
            state.message_scroll = state.max_message_scroll();
            state.pending_auto_scroll = false;
        }
        assert_eq!(state.message_scroll, 19);

        state.message_scroll = 5;
        state.user_scrolled_up = true;
        state.pending_auto_scroll = true;
        if state.pending_auto_scroll && !state.user_scrolled_up {
            state.message_scroll = state.max_message_scroll();
        }
        assert_eq!(state.message_scroll, 5);
    }
}
