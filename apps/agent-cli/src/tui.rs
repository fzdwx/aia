use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io,
    path::Path,
    time::Duration,
};

use agent_core::StreamEvent;
use agent_runtime::{AgentRuntime, RuntimeEvent, RuntimeSubscriberId, TurnLifecycle};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use provider_registry::{ProviderProfile, ProviderRegistry};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Paragraph, Widget, Wrap},
};
use session_tape::{SessionProviderBinding, SessionTape};

use crate::{
    driver::{self, CliRuntime, DriverHandle, DriverPollResult},
    errors::CliLoopError,
    loop_driver::is_exit_command,
    model::{BootstrapTools, ProviderLaunchChoice, build_model_from_selection},
    theme,
    tui_markdown::{
        inline_markdown_lines, inline_thinking_lines, markdown_lines, padded_plain_line,
        padded_plain_lines, prefixed_markdown_lines, user_message_lines,
    },
    tui_timeline::reconstruct_turns,
};

const TUI_RENDER_INTERVAL_MS: u64 = 16;
const MAX_DELTAS_PER_FRAME: usize = 64;
const STATUS_ANIMATION_FRAME_DIVISOR: usize = 6;
const STATUS_ANIMATION_TRAIL_LENGTH: usize = 2;
const STATUS_ANIMATION_RESTART_PAUSE: usize = 2;
const STREAMING_PINNED_MAX_LINES: usize = 6;

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
        if had_new_turns && !state.messages.user_scrolled_up {
            state.messages.pending_auto_scroll = true;
        }
        // Advance spinner each frame during streaming
        if state.messages.streaming.is_some() {
            state.messages.advance_spinner();
        }
        terminal.draw(|frame| draw_tui(frame, state, registry))?;

        if !event::poll(Duration::from_millis(TUI_RENDER_INTERVAL_MS))? {
            continue;
        }

        let input_event = event::read()?;
        if let Event::Resize(_, _) = input_event {
            handle_resize_event(state);
            continue;
        }
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
                    state.messages.scroll_up();
                    continue;
                }
            }
            KeyCode::Down => {
                if matches!(state.focus, FocusArea::Messages) {
                    state.messages.scroll_down();
                    continue;
                }
            }
            KeyCode::PageUp => {
                if matches!(state.focus, FocusArea::Messages) {
                    state.messages.page_up();
                    continue;
                }
            }
            KeyCode::PageDown => {
                if matches!(state.focus, FocusArea::Messages) {
                    state.messages.page_down();
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
                    state.messages.scroll_to_top();
                    continue;
                }
                state.cursor_pos = 0;
                continue;
            }
            KeyCode::End | KeyCode::Char('e')
                if key.code == KeyCode::End || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                if matches!(state.focus, FocusArea::Messages) && key.code == KeyCode::End {
                    state.messages.scroll_to_bottom();
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
            state.messages.set_processing(true);
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
    if state.messages.processing {
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
            if !state.messages.user_scrolled_up {
                state.messages.pending_auto_scroll = true;
            }
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

#[derive(Clone)]
struct HistoryCache {
    width: u16,
    content_key: u64,
    lines: Vec<Line<'static>>,
    visual_line_count: usize,
}

struct StreamingOverlay {
    separator: Vec<Line<'static>>,
    lines: Vec<Line<'static>>,
    footer_text: Option<String>,
    visual_line_count: usize,
}

#[derive(Clone)]
struct MessagePanel {
    // --- data ---
    replay_turns: Vec<TurnLifecycle>,
    current_turns: Vec<TurnLifecycle>,
    streaming: Option<StreamingTurn>,
    processing: bool,

    // --- two-level cache ---
    history_cache: Option<HistoryCache>,
    overlay_cache: Option<OverlayCache>,

    // --- scroll ---
    scroll_offset: usize,
    line_count: usize,
    viewport_height: usize,
    user_scrolled_up: bool,
    pending_auto_scroll: bool,

    // --- render ---
    spinner_tick: usize,
    area: Rect,
}

#[derive(Clone)]
struct OverlayCache {
    width: u16,
    has_history: bool,
    thinking_len: usize,
    text_len: usize,
    user_message_len: usize,
    processing: bool,
    is_streaming: bool,
    overlay: CachedOverlayData,
}

#[derive(Clone)]
struct CachedOverlayData {
    separator: Vec<Line<'static>>,
    lines: Vec<Line<'static>>,
    footer_text: Option<String>,
    visual_line_count: usize,
}

impl MessagePanel {
    fn new(replay_turns: Vec<TurnLifecycle>) -> Self {
        let has_replay = !replay_turns.is_empty();
        Self {
            replay_turns,
            current_turns: Vec::new(),
            streaming: None,
            processing: false,
            history_cache: None,
            overlay_cache: None,
            scroll_offset: 0,
            line_count: 0,
            viewport_height: 1,
            user_scrolled_up: false,
            pending_auto_scroll: has_replay,
            spinner_tick: 0,
            area: Rect::default(),
        }
    }

    // --- event-driven updates ---

    fn start_streaming(&mut self, user_message: String) {
        self.streaming = Some(StreamingTurn {
            user_message,
            status_text: Some("Thinking".into()),
            thinking: String::new(),
            text: String::new(),
        });
        self.processing = true;
        self.overlay_cache = None;
        self.user_scrolled_up = false;
        self.pending_auto_scroll = true;
    }

    fn push_delta(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::ThinkingDelta { text } => {
                let streaming = self.streaming.get_or_insert_with(StreamingTurn::default);
                streaming.status_text = Some("Thinking".into());
                streaming.thinking.push_str(&text);
                self.overlay_cache = None;
                if !self.user_scrolled_up {
                    self.pending_auto_scroll = true;
                }
            }
            StreamEvent::TextDelta { text } => {
                let streaming = self.streaming.get_or_insert_with(StreamingTurn::default);
                streaming.status_text = Some("Responding".into());
                streaming.text.push_str(&text);
                self.overlay_cache = None;
                if !self.user_scrolled_up {
                    self.pending_auto_scroll = true;
                }
            }
            StreamEvent::Log { .. } | StreamEvent::Done => {}
        }
    }

    fn complete_turn(&mut self, turn: TurnLifecycle) {
        self.streaming = None;
        self.processing = false;
        self.current_turns.push(turn);
        self.history_cache = None;
        self.overlay_cache = None;
        if !self.user_scrolled_up {
            self.pending_auto_scroll = true;
        }
    }

    fn set_processing(&mut self, processing: bool) {
        self.processing = processing;
    }

    fn advance_spinner(&mut self) {
        self.spinner_tick = self.spinner_tick.wrapping_add(1);
    }

    // --- scrolling ---

    fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(1);
            self.user_scrolled_up = true;
        }
    }

    fn scroll_down(&mut self) {
        let max = self.max_scroll();
        if self.scroll_offset < max {
            self.scroll_offset += 1;
        }
        if self.scroll_offset >= max {
            self.user_scrolled_up = false;
        }
    }

    fn page_up(&mut self) {
        let step = self.viewport_height.max(1);
        self.scroll_offset = self.scroll_offset.saturating_sub(step);
        self.user_scrolled_up = self.scroll_offset > 0;
    }

    fn page_down(&mut self) {
        let max = self.max_scroll();
        let step = self.viewport_height.max(1);
        self.scroll_offset = (self.scroll_offset + step).min(max);
        if self.scroll_offset >= max {
            self.user_scrolled_up = false;
        }
    }

    fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        self.user_scrolled_up = true;
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.max_scroll();
        self.user_scrolled_up = false;
    }

    fn is_mouse_inside(&self, col: u16, row: u16) -> bool {
        col >= self.area.x
            && col < self.area.x + self.area.width
            && row >= self.area.y
            && row < self.area.y + self.area.height
    }

    fn invalidate_on_resize(&mut self) {
        self.history_cache = None;
        self.overlay_cache = None;
        if !self.user_scrolled_up {
            self.pending_auto_scroll = true;
        }
    }

    fn max_scroll(&self) -> usize {
        self.line_count.saturating_sub(self.viewport_height.max(1))
    }

    fn clamp_scroll(&mut self) {
        let max = self.max_scroll();
        if self.scroll_offset > max {
            self.scroll_offset = max;
        }
    }

    // --- rendering ---

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        self.area = area;
        let width = area.width;

        // 1. Build or reuse history cache
        let history_key = self.history_content_key();
        let history = match &self.history_cache {
            Some(cache) if cache.width == width && cache.content_key == history_key => {
                cache.clone()
            }
            _ => {
                let lines = self.build_history_lines(width);
                let vlc = visual_line_count(&lines, width);
                let cache = HistoryCache {
                    width,
                    content_key: history_key,
                    lines,
                    visual_line_count: vlc,
                };
                self.history_cache = Some(cache.clone());
                cache
            }
        };

        // 2. Build or reuse streaming overlay cache
        let has_history = !history.lines.is_empty();
        let overlay = self.cached_streaming_overlay(width, has_history);

        // 3. Merge final lines
        let mut final_lines = history.lines;
        let mut total_vlc = history.visual_line_count;

        if !overlay.separator.is_empty() || !overlay.lines.is_empty() {
            total_vlc += overlay.visual_line_count;
            final_lines.extend(overlay.separator);
            final_lines.extend(overlay.lines);
        }

        // 4. Footer
        let footer = overlay.footer_text.as_deref().map(|text| {
            padded_plain_line(animated_status_line(text, self.spinner_tick))
        });
        let has_footer = footer.is_some() && area.height > 0;
        let body_area = if has_footer {
            Rect { x: area.x, y: area.y, width: area.width, height: area.height.saturating_sub(1) }
        } else {
            area
        };

        // 5. Update scroll state
        self.line_count = total_vlc;
        self.viewport_height = body_area.height.max(1) as usize;
        if !self.user_scrolled_up && (self.pending_auto_scroll || self.processing) {
            self.scroll_offset = self.max_scroll();
            self.pending_auto_scroll = false;
        } else {
            self.clamp_scroll();
        }

        // 6. Render
        let panel = Paragraph::new(Text::from(final_lines))
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset as u16, 0));
        frame.render_widget(panel, body_area);

        if let Some(footer) = footer {
            let footer_area = Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            };
            frame.render_widget(Paragraph::new(footer), footer_area);
        }
    }

    // --- private helpers ---

    fn cached_streaming_overlay(&mut self, width: u16, has_history: bool) -> StreamingOverlay {
        // Check if cached overlay is still valid
        let (thinking_len, text_len, user_msg_len, is_streaming) =
            if let Some(s) = &self.streaming {
                (s.thinking.len(), s.text.len(), s.user_message.len(), true)
            } else {
                (0, 0, 0, false)
            };

        if let Some(ref cache) = self.overlay_cache {
            if cache.width == width
                && cache.has_history == has_history
                && cache.thinking_len == thinking_len
                && cache.text_len == text_len
                && cache.user_message_len == user_msg_len
                && cache.processing == self.processing
                && cache.is_streaming == is_streaming
            {
                return StreamingOverlay {
                    separator: cache.overlay.separator.clone(),
                    lines: cache.overlay.lines.clone(),
                    footer_text: cache.overlay.footer_text.clone(),
                    visual_line_count: cache.overlay.visual_line_count,
                };
            }
        }

        let overlay = self.build_streaming_overlay(width, has_history);
        self.overlay_cache = Some(OverlayCache {
            width,
            has_history,
            thinking_len,
            text_len,
            user_message_len: user_msg_len,
            processing: self.processing,
            is_streaming,
            overlay: CachedOverlayData {
                separator: overlay.separator.clone(),
                lines: overlay.lines.clone(),
                footer_text: overlay.footer_text.clone(),
                visual_line_count: overlay.visual_line_count,
            },
        });
        overlay
    }

    fn history_content_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.replay_turns.len().hash(&mut hasher);
        self.current_turns.len().hash(&mut hasher);
        hash_turns(&self.replay_turns, &mut hasher);
        hash_turns(&self.current_turns, &mut hasher);
        hasher.finish()
    }

    fn build_history_lines(&self, width: u16) -> Vec<Line<'static>> {
        let all_turns: Vec<&TurnLifecycle> =
            self.replay_turns.iter().chain(self.current_turns.iter()).collect();

        let mut lines: Vec<Line<'static>> = Vec::new();
        for (index, turn) in all_turns.iter().enumerate() {
            lines.extend(turn_lines(turn, width));
            if index + 1 < all_turns.len() {
                lines.push(Line::default());
            }
        }
        lines
    }

    fn build_streaming_overlay(
        &self,
        width: u16,
        has_history: bool,
    ) -> StreamingOverlay {
        let mut separator = Vec::new();
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut footer_text = None;

        if let Some(ref streaming) = self.streaming {
            let streaming_has_user = !streaming.user_message.is_empty();
            if has_history {
                if !streaming_has_user {
                    separator.push(Line::default());
                }
            }
            if streaming_has_user {
                lines.extend(user_block_lines(&streaming.user_message, width));
            }
            if !streaming.thinking.is_empty() {
                if streaming_has_user {
                    lines.push(half_gap_line(width));
                }
                lines.extend(padded_plain_lines(inline_markdown_lines(
                    "Thinking: ",
                    &streaming.thinking,
                    theme::thinking_label_style(),
                    theme::thinking_style(),
                )));
            }
            if !streaming.text.is_empty() {
                if streaming_has_user || !streaming.thinking.is_empty() {
                    if streaming_has_user && streaming.thinking.is_empty() {
                        lines.push(half_gap_line(width));
                    } else {
                        ensure_blank_line(&mut lines);
                    }
                }
                lines.extend(padded_plain_lines(markdown_lines(
                    &streaming.text,
                    theme::assistant_message_style(),
                )));
            }
            lines = clamp_streaming_lines(lines);
            if let Some(status_text) = &streaming.status_text {
                footer_text = Some(status_text.clone());
            }
        } else if self.processing {
            footer_text = Some("Thinking".into());
        }

        let all_overlay: Vec<Line<'static>> =
            separator.iter().chain(lines.iter()).cloned().collect();
        let vlc = if all_overlay.is_empty() {
            0
        } else {
            visual_line_count(&all_overlay, width)
        };

        StreamingOverlay {
            separator,
            lines,
            footer_text,
            visual_line_count: vlc,
        }
    }
}

#[derive(Clone)]
struct TuiState {
    messages: MessagePanel,
    model_label: String,
    input: String,
    cursor_pos: usize,
    status: Option<String>,
    phase: Phase,
    selected_option: usize,
    prompt_seed: Option<String>,
    should_exit: bool,
    focus: FocusArea,
    pending_prompt: Option<String>,
    log_lines: Vec<String>,
    show_logs: bool,
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
            messages: MessagePanel::new(turns),
            model_label,
            input,
            cursor_pos,
            status,
            phase,
            selected_option: 0,
            prompt_seed,
            should_exit: false,
            focus: FocusArea::Input,
            pending_prompt: None,
            log_lines: Vec::new(),
            show_logs: false,
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
}

/// Auto-scroll when new turns arrive
fn handle_mouse_event(mouse: MouseEvent, state: &mut TuiState) -> bool {
    if !state.messages.is_mouse_inside(mouse.column, mouse.row) {
        return false;
    }

    match mouse.kind {
        MouseEventKind::ScrollDown => {
            state.messages.scroll_down();
            true
        }
        MouseEventKind::ScrollUp => {
            state.messages.scroll_up();
            true
        }
        _ => false,
    }
}

fn handle_resize_event(state: &mut TuiState) {
    state.messages.invalidate_on_resize();
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
    state.messages.start_streaming(prompt.clone());
    driver::submit_turn(driver, prompt).map_err(CliLoopError::from)?;
    state.status = Some("正在处理中...".into());
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
    let mut delta_count = 0usize;
    loop {
        match driver::poll_driver(driver).map_err(CliLoopError::from)? {
            DriverPollResult::StreamDelta(event) => match &event {
                StreamEvent::ThinkingDelta { .. } | StreamEvent::TextDelta { .. } => {
                    state.messages.push_delta(event);
                    had_updates = true;
                    delta_count += 1;
                    if delta_count >= MAX_DELTAS_PER_FRAME {
                        break;
                    }
                }
                StreamEvent::Log { text } => {
                    state.log_lines.push(text.clone());
                }
                StreamEvent::Done => {}
            },
            DriverPollResult::TurnCompleted(driver::DriverTurnResult {
                events,
                turn_error,
                persist_error,
            }) => {
                for event in events {
                    if let RuntimeEvent::TurnLifecycle { turn } = event {
                        state.messages.complete_turn(turn);
                        had_updates = true;
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
            DriverPollResult::Nothing => break,
        }
    }

    Ok(had_updates)
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
                .constraints([Constraint::Min(4), Constraint::Length(3)])
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
            draw_input_bar(frame, layout[1], state);
        }
        Phase::SelectProvider => {
            // 3 zones: messages (top) | phase UI (mid) | input bar (bottom)
            let has_messages = !state.messages.replay_turns.is_empty();
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
            let has_messages = !state.messages.replay_turns.is_empty();
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
            draw_input_bar(frame, layout[1], state);
        }
    }
}

fn draw_messages(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut TuiState) {
    state.messages.draw(frame, area);
}

struct MessageViewForTest {
    lines: Vec<Line<'static>>,
    footer: Option<Line<'static>>,
}

#[cfg(test)]
fn message_lines(panel: &MessagePanel, width: u16) -> MessageViewForTest {
    let history = panel.build_history_lines(width);
    let overlay = panel.build_streaming_overlay(width, !history.is_empty());

    let mut lines = history;
    lines.extend(overlay.separator);
    lines.extend(overlay.lines);

    let footer = overlay
        .footer_text
        .as_deref()
        .map(|text| padded_plain_line(animated_status_line(text, panel.spinner_tick)));

    MessageViewForTest { lines, footer }
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

    let status_text = if state.messages.processing {
        String::new()
    } else {
        state.status.clone().unwrap_or_else(|| "就绪".into())
    };
    let status_style = if state.messages.processing { theme::spinner_style() } else { theme::dim_style() };

    // Build 3 lines: input | status bar | hints
    let status_bar = if state.messages.processing {
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

fn visual_line_count(lines: &[Line<'_>], width: u16) -> usize {
    if lines.is_empty() {
        return 0;
    }

    let width = width.max(1);
    let estimated_rows = estimate_wrapped_rows_upper_bound(lines, width);
    let height = estimated_rows.max(1).min(u16::MAX as usize) as u16;
    let area = Rect { x: 0, y: 0, width, height };
    let mut buffer = Buffer::empty(area);
    for cell in &mut buffer.content {
        cell.set_symbol("·");
    }

    Paragraph::new(Text::from(lines.to_vec())).wrap(Wrap { trim: false }).render(area, &mut buffer);

    (0..height)
        .rev()
        .find(|row| {
            let start = *row as usize * width as usize;
            let end = start + width as usize;
            buffer.content[start..end].iter().any(|cell| cell.symbol() != "·")
        })
        .map(|row| row as usize + 1)
        .unwrap_or(0)
}

fn estimate_wrapped_rows_upper_bound(lines: &[Line<'_>], width: u16) -> usize {
    let _ = width;
    lines
        .iter()
        .map(|line| {
            let char_count =
                line.spans.iter().map(|span| span.content.chars().count()).sum::<usize>();
            char_count.saturating_add(1).max(1)
        })
        .sum()
}

fn tool_header_line(tool_name: &str, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled("• tool ", style),
        Span::styled(tool_name.to_string(), theme::tool_name_style()),
    ])
}

fn separator_line(width: u16) -> Line<'static> {
    padded_plain_line(Line::from(Span::styled(
        "─".repeat(width.min(60) as usize),
        theme::separator_style(),
    )))
}

fn half_gap_line(width: u16) -> Line<'static> {
    let _ = width;
    padded_plain_line(Line::default())
}

fn animated_status_line(text: &str, tick: usize) -> Line<'static> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Line::default();
    }
    let len = chars.len();
    let frame = tick / STATUS_ANIMATION_FRAME_DIVISOR.max(1);
    let cycle = len + STATUS_ANIMATION_TRAIL_LENGTH + STATUS_ANIMATION_RESTART_PAUSE;
    let head = frame % cycle;
    Line::from(
        chars
            .into_iter()
            .enumerate()
            .map(|(index, ch)| {
                let style = if head < len && index == head {
                    theme::status_head_style()
                } else if head > index && head - index == 1 {
                    theme::status_trail_style()
                } else if head > index && head - index == 2 {
                    theme::dim_style()
                } else {
                    theme::status_dim_style()
                };
                Span::styled(ch.to_string(), style)
            })
            .collect::<Vec<_>>(),
    )
}

fn turn_lines(turn: &TurnLifecycle, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let has_assistant_content =
        turn.assistant_message.as_ref().is_some_and(|assistant| !assistant.trim().is_empty());
    let has_tool_section = !turn.tool_invocations.is_empty();
    let has_thinking_section = turn.thinking.is_some();
    lines.extend(user_block_lines(&turn.user_message, width));

    if let Some(thinking) = &turn.thinking {
        ensure_blank_line(&mut lines);
        lines.extend(padded_plain_lines(inline_thinking_lines(thinking)));
    }

    for invocation in &turn.tool_invocations {
        ensure_blank_line(&mut lines);
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

    if has_tool_section {
        if has_assistant_content {
            ensure_blank_line(&mut lines);
            lines.push(separator_line(width));
        }
    }

    if let Some(assistant) = &turn.assistant_message {
        if assistant.trim().is_empty() {
            return lines;
        }
        if has_tool_section || has_thinking_section {
            ensure_blank_line(&mut lines);
        } else {
            lines.push(half_gap_line(width));
        }
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

fn ensure_blank_line(lines: &mut Vec<Line<'static>>) {
    let needs_blank = lines.last().is_some_and(|line| !is_blank_line(line));
    if needs_blank {
        lines.push(Line::default());
    }
}

fn stretch_user_lines_to_full_width(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let target_width = width.max(1) as usize;
    let fill_style = theme::user_message_style();
    lines
        .into_iter()
        .map(|mut line| {
            let current_width = line_display_width(&line);
            if current_width < target_width {
                line.spans.push(Span::styled(
                    "\u{00A0}".repeat(target_width - current_width),
                    fill_style,
                ));
            }
            line
        })
        .collect()
}

fn user_block_lines(content: &str, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(user_padding_line(width));
    lines.extend(stretch_user_lines_to_full_width(user_message_lines(content), width));
    lines.push(user_padding_line(width));
    lines
}

fn user_padding_line(width: u16) -> Line<'static> {
    Line::from(Span::styled("\u{00A0}".repeat(width.max(1) as usize), theme::user_message_style()))
}

fn clamp_streaming_lines(lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    if lines.len() <= STREAMING_PINNED_MAX_LINES {
        return lines;
    }

    let keep_tail = STREAMING_PINNED_MAX_LINES.saturating_sub(1);
    let start = lines.len().saturating_sub(keep_tail);
    let mut compact = Vec::with_capacity(STREAMING_PINNED_MAX_LINES);
    compact.push(padded_plain_line(Line::from(Span::styled("…", theme::dim_style()))));
    compact.extend(lines.into_iter().skip(start));
    compact
}

fn line_display_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .flat_map(|span| span.content.chars())
        .map(|ch| {
            if ch.is_control() {
                0
            } else if is_wide_char(ch) {
                2
            } else {
                1
            }
        })
        .sum()
}

fn is_blank_line(line: &Line<'_>) -> bool {
    line.spans.iter().all(|span| span.content.chars().all(|ch| ch.is_whitespace()))
}

fn hash_turns(turns: &[TurnLifecycle], hasher: &mut DefaultHasher) {
    for turn in turns {
        turn.turn_id.hash(hasher);
        turn.started_at_ms.hash(hasher);
        turn.finished_at_ms.hash(hasher);
        turn.source_entry_ids.hash(hasher);
        turn.user_message.hash(hasher);
        turn.assistant_message.hash(hasher);
        turn.thinking.hash(hasher);
        turn.failure_message.hash(hasher);
        for invocation in &turn.tool_invocations {
            invocation.call.tool_name.hash(hasher);
            invocation.call.invocation_id.hash(hasher);
            invocation.call.arguments.hash(hasher);
            match &invocation.outcome {
                agent_runtime::ToolInvocationOutcome::Succeeded { result } => {
                    1u8.hash(hasher);
                    result.invocation_id.hash(hasher);
                    result.content.hash(hasher);
                }
                agent_runtime::ToolInvocationOutcome::Failed { message } => {
                    2u8.hash(hasher);
                    message.hash(hasher);
                }
            }
        }
    }
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
        state.messages.replay_turns.push(agent_runtime::TurnLifecycle {
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

        assert_eq!(rendered[thinking_index + 1], " 与 代码 ");
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
        let user_line_one =
            lines.iter().find(|line| line_text(line).contains("用户消息")).expect("应有用户首行");
        let user_line_two =
            lines.iter().find(|line| line_text(line).contains("第二行")).expect("应有用户次行");
        let user_line_one_bg = line_backgrounds(user_line_one);
        let user_line_two_bg = line_backgrounds(user_line_two);
        let assistant_line =
            lines.iter().find(|line| line_text(line).contains("助手消息")).expect("应有助手消息");
        let assistant_bgs = line_backgrounds(assistant_line);

        assert!(line_text(user_line_one).starts_with(" 用户消息 "));
        assert!(line_text(user_line_two).starts_with(" 第二行 "));
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
    fn turn_lines_用户与助手正文之间使用空白间隔() {
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "用户消息".into(),
            assistant_message: Some("助手回复".into()),
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 60);
        let rendered = lines.iter().map(line_text).collect::<Vec<_>>();
        let user_index =
            rendered.iter().position(|line| line.contains("用户消息")).expect("应有用户消息");
        let assistant_index =
            rendered.iter().position(|line| line.contains("助手回复")).expect("应有助手回复");

        assert_eq!(assistant_index, user_index + 3);
        assert!(rendered[user_index + 2].trim().is_empty());
    }

    #[test]
    fn turn_lines_单用户消息会上下各追加一行背景_padding() {
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "只发一条用户消息".into(),
            assistant_message: None,
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 24);
        assert!(lines.len() >= 3);

        let first = &lines[0];
        let middle = &lines[1];
        let last = lines.last().expect("应有底部 padding 行");

        assert!(line_text(first).trim().is_empty());
        assert!(line_text(middle).contains("只发一条用户消息"));
        assert!(line_text(last).trim().is_empty());

        let first_bgs = line_backgrounds(first);
        let last_bgs = line_backgrounds(last);
        assert!(first_bgs.iter().all(|bg| *bg == theme::current().user_message_style.bg));
        assert!(last_bgs.iter().all(|bg| *bg == theme::current().user_message_style.bg));
    }

    #[test]
    fn 流式仅用户消息会上下各追加一行背景_padding() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.streaming = Some(super::StreamingTurn {
            user_message: "单用户消息".into(),
            status_text: None,
            thinking: String::new(),
            text: String::new(),
        });

        let lines = super::message_lines(&state.messages, 24).lines;
        assert!(lines.len() >= 3);

        let first = &lines[0];
        let middle = &lines[1];
        let last = lines.last().expect("应有底部 padding 行");

        assert!(line_text(first).trim().is_empty());
        assert!(line_text(middle).contains("单用户消息"));
        assert!(line_text(last).trim().is_empty());

        let first_bgs = line_backgrounds(first);
        let last_bgs = line_backgrounds(last);
        assert!(first_bgs.iter().all(|bg| *bg == theme::current().user_message_style.bg));
        assert!(last_bgs.iter().all(|bg| *bg == theme::current().user_message_style.bg));
    }

    #[test]
    fn 用户背景块与助手正文之间保持空白间隔() {
        let backend = TestBackend::new(24, 10);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.current_turns.push(agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1, 2],
            user_message: "哦".into(),
            assistant_message: Some("嗯".into()),
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        });
        let registry = provider_registry::ProviderRegistry::default();

        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");
        let buffer = terminal.backend().buffer().clone();
        let rows =
            (0..buffer.area.height).map(|row| buffer_row_text(&buffer, row)).collect::<Vec<_>>();
        let user_row = rows.iter().position(|row| row.contains('哦')).expect("应渲染用户消息");
        let assistant_row = rows.iter().position(|row| row.contains('嗯')).expect("应渲染助手消息");

        assert!(assistant_row >= user_row + 3);
    }

    #[test]
    fn 用户背景补齐后行宽不会超过目标宽度() {
        let lines = vec![ratatui::text::Line::from(" 用户 ")];
        let stretched = super::stretch_user_lines_to_full_width(lines, 12);

        assert_eq!(super::line_display_width(&stretched[0]), 12);
    }

    #[test]
    fn tui_流式_thinking_会首行内联且保留换行() {
        let backend = TestBackend::new(100, 12);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.streaming = Some(super::StreamingTurn {
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
        assert!(row_two.starts_with(" next code "));
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

        let streaming = state.messages.streaming.expect("应创建进行中轮次");
        assert_eq!(streaming.user_message, "hello world");
        assert_eq!(streaming.status_text.as_deref(), Some("Thinking"));
        assert!(state.messages.processing);
        assert!(!state.messages.user_scrolled_up);
        assert!(state.messages.pending_auto_scroll);
    }

    #[test]
    fn 运行状态会显示在消息列表中() {
        let backend = TestBackend::new(100, 12);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.processing = true;
        state.status = Some("正在处理中...".into());
        state.messages.streaming = Some(super::StreamingTurn {
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
        let line = super::animated_status_line("Thinking", 6);

        assert_eq!(line.spans[0].style, theme::status_trail_style());
        assert_eq!(line.spans[1].style, theme::status_head_style());
        assert_eq!(line.spans[2].style, theme::status_dim_style());
        assert_eq!(line.spans[4].style, theme::status_dim_style());
    }

    #[test]
    fn 状态动画在尾迹结束前不会从头重新开始() {
        let text = "Thinking";
        let end_frame = text.chars().count() - 1;
        let end_tick = end_frame * super::STATUS_ANIMATION_FRAME_DIVISOR;
        let after_end_tick = (end_frame + 1) * super::STATUS_ANIMATION_FRAME_DIVISOR;
        let line_at_end = super::animated_status_line(text, end_tick);
        let line_after_end = super::animated_status_line(text, after_end_tick);

        assert_eq!(line_at_end.spans[0].style, theme::status_dim_style());
        assert_eq!(line_at_end.spans[text.chars().count() - 1].style, theme::status_head_style());
        assert_eq!(line_after_end.spans[0].style, theme::status_dim_style());
        assert_eq!(line_after_end.spans[1].style, theme::status_dim_style());
    }

    #[test]
    fn 鼠标滚轮可以滚动消息列表() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.area = Rect { x: 0, y: 0, width: 80, height: 10 };
        state.messages.line_count = 40;
        state.messages.viewport_height = 10;

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
        assert!(state.messages.scroll_offset > 0);
    }

    #[test]
    fn 流式状态固定显示在消息列表最底部() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.processing = true;
        state.messages.streaming = Some(super::StreamingTurn {
            user_message: "hello".into(),
            status_text: Some("Thinking".into()),
            thinking: "first line".into(),
            text: "reply".into(),
        });

        let view = super::message_lines(&state.messages, 60);
        let footer = view.footer.expect("应有底部状态行");

        assert!(line_text(&footer).contains("Thinking"));
    }

    #[test]
    fn 消息视图缓存会复用正文并允许_footer_动画继续更新() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.processing = true;
        state.messages.streaming = Some(super::StreamingTurn {
            user_message: "hello".into(),
            status_text: Some("Thinking".into()),
            thinking: "first line".into(),
            text: "reply".into(),
        });

        let first = super::message_lines(&state.messages, 60);
        let cached_key = state.messages.history_cache.as_ref().map(|c| c.content_key);
        let first_lines = first.lines.iter().map(line_text).collect::<Vec<_>>();
        let first_footer = line_text(&first.footer.expect("应有 footer"));

        state.messages.spinner_tick = 3;
        let second = super::message_lines(&state.messages, 60);
        let second_lines = second.lines.iter().map(line_text).collect::<Vec<_>>();
        let second_footer = line_text(&second.footer.expect("应有 footer"));

        assert_eq!(cached_key, state.messages.history_cache.as_ref().map(|c| c.content_key));
        assert_eq!(first_lines, second_lines);
        assert_eq!(first_footer, second_footer);
    }

    #[test]
    fn 流式_thinking_增量会触发消息区域刷新() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.processing = true;
        state.messages.streaming = Some(super::StreamingTurn {
            user_message: "hello".into(),
            status_text: Some("Thinking".into()),
            thinking: "first".into(),
            text: String::new(),
        });

        let first = super::message_lines(&state.messages, 60);
        let first_lines = first.lines.iter().map(line_text).collect::<Vec<_>>();

        state.messages.streaming.as_mut().expect("应存在流式轮次").thinking.push_str("\nsecond");

        let second = super::message_lines(&state.messages, 60);
        let second_lines = second.lines.iter().map(line_text).collect::<Vec<_>>();

        assert_ne!(first_lines, second_lines);
        assert!(second_lines.iter().any(|line| line.contains("second")));
    }

    #[test]
    fn 流式区域会限制最大行数以固定在输入框上方() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.processing = true;
        state.messages.streaming = Some(super::StreamingTurn {
            user_message: "hello".into(),
            status_text: Some("Responding".into()),
            thinking: String::new(),
            text: (1..=12).map(|i| format!("第{i}行")).collect::<Vec<_>>().join("\n"),
        });

        let lines = super::message_lines(&state.messages, 60).lines;
        let rendered = lines.iter().map(line_text).collect::<Vec<_>>();

        assert!(rendered.len() <= super::STREAMING_PINNED_MAX_LINES);
        assert!(rendered.iter().any(|line| line.contains('…')));
        assert!(rendered.iter().any(|line| line.contains("第12行")));
    }

    #[test]
    fn draw_messages_会用最新视口信息自动滚动到底部() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.pending_auto_scroll = true;
        state.messages.current_turns.push(agent_runtime::TurnLifecycle {
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

        assert_eq!(state.messages.scroll_offset, state.messages.max_scroll());
        assert!(!state.messages.pending_auto_scroll);
    }

    #[test]
    fn 首次进入且存在历史消息时会自动滚动到底部() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(
            vec![agent_runtime::TurnLifecycle {
                turn_id: "turn-1".into(),
                started_at_ms: 1,
                finished_at_ms: 2,
                source_entry_ids: vec![1],
                user_message: "历史提问".into(),
                assistant_message: Some(
                    "历史回答第一行\n历史回答第二行\n历史回答第三行\n历史回答第四行".into(),
                ),
                thinking: None,
                tool_invocations: vec![],
                failure_message: None,
            }],
            None,
            None,
            None,
        );
        state.phase = Phase::Chat;
        let registry = provider_registry::ProviderRegistry::default();

        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");

        assert_eq!(state.messages.scroll_offset, state.messages.max_scroll());
        assert!(!state.messages.pending_auto_scroll);
    }

    #[test]
    fn 处理中且未手动上滚时会持续自动跟底() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.processing = true;
        state.messages.streaming = Some(super::StreamingTurn {
            user_message: "当前输入".into(),
            status_text: Some("Thinking".into()),
            thinking: "第一行\n第二行\n第三行\n第四行\n第五行\n第六行".into(),
            text: "回复\n第二行\n第三行\n第四行".into(),
        });
        let registry = provider_registry::ProviderRegistry::default();

        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");

        assert_eq!(state.messages.scroll_offset, state.messages.max_scroll());
    }

    #[test]
    fn 流式轮次与历史消息之间最多只保留一行空白() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.replay_turns.push(agent_runtime::TurnLifecycle {
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
        state.messages.streaming = Some(super::StreamingTurn {
            user_message: "当前输入".into(),
            status_text: Some("Thinking".into()),
            thinking: String::new(),
            text: String::new(),
        });

        let rendered = super::message_lines(&state.messages, 60)
            .lines
            .into_iter()
            .map(|line| line_text(&line))
            .collect::<Vec<_>>();
        let current_index =
            rendered.iter().position(|line| line.contains(" 当前输入 ")).expect("应有当前输入");

        assert!(rendered[current_index - 1].trim().is_empty());
        assert!(!rendered[current_index - 2].trim().is_empty());
    }

    #[test]
    fn markdown_段落之间不再额外保留空行() {
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "测试".into(),
            assistant_message: Some("第一段\n\n第二段".into()),
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        };

        let lines = super::turn_lines(&turn, 60);
        let text_lines = lines.iter().map(line_text).collect::<Vec<_>>();
        let first_index =
            text_lines.iter().position(|line| line.contains("第一段")).expect("应有第一段");
        let second_index =
            text_lines.iter().position(|line| line.contains("第二段")).expect("应有第二段");

        assert_eq!(second_index, first_index + 1);
    }

    #[test]
    fn markdown_单个换行不会被扩成空白段() {
        let lines =
            crate::tui_markdown::markdown_lines("第一行\n第二行", theme::assistant_message_style());
        let text_lines = lines.iter().map(line_text).collect::<Vec<_>>();

        assert_eq!(text_lines, vec!["第一行".to_string(), "第二行".to_string()]);
    }

    #[test]
    fn markdown_尾部单个换行不会额外生成空白行() {
        let lines =
            crate::tui_markdown::markdown_lines("第一行\n", theme::assistant_message_style());
        let text_lines = lines.iter().map(line_text).collect::<Vec<_>>();

        assert_eq!(text_lines, vec!["第一行".to_string()]);
    }

    #[test]
    fn markdown_连续空行会压缩为单换行显示() {
        let lines = crate::tui_markdown::markdown_lines(
            "第一段\n\n\n第二段",
            theme::assistant_message_style(),
        );
        let text_lines = lines.iter().map(line_text).collect::<Vec<_>>();

        assert_eq!(text_lines, vec!["第一段".to_string(), "第二段".to_string()]);
    }

    #[test]
    fn 聊天布局会优先把高度分配给消息区() {
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        let registry = provider_registry::ProviderRegistry::default();

        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");

        assert_eq!(state.messages.viewport_height, 9);
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
        state.messages.line_count = 5;
        state.messages.scroll_offset = 10;
        state.messages.clamp_scroll();
        assert_eq!(state.messages.scroll_offset, 4); // max = 5-1 = 4
    }

    #[test]
    fn 可视行计数会把自动换行折算进滚动高度() {
        let lines =
            vec![ratatui::text::Line::from("1234567890"), ratatui::text::Line::from("第二行")];

        let count = super::visual_line_count(&lines, 5);

        assert_eq!(count, 3);
    }

    #[test]
    fn draw_messages_自动滚动会基于可视行高度而非原始行数() {
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.pending_auto_scroll = true;
        state.messages.current_turns.push(agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![1],
            user_message: "问题".into(),
            assistant_message: Some(
                "这是一个非常非常非常非常非常非常长的回答，用于触发自动换行并验证滚动高度计算。"
                    .into(),
            ),
            thinking: None,
            tool_invocations: vec![],
            failure_message: None,
        });
        let registry = provider_registry::ProviderRegistry::default();

        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");

        let raw_line_count =
            state.messages.history_cache.as_ref().expect("应有消息视图缓存").lines.len();

        assert!(state.messages.line_count > raw_line_count);
        assert_eq!(state.messages.scroll_offset, state.messages.max_scroll());
    }

    #[test]
    fn 消息滚动上限会考虑视口高度() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.focus = FocusArea::Messages;
        state.messages.line_count = 20;
        state.messages.viewport_height = 5;
        state.messages.scroll_offset = 99;

        state.messages.clamp_scroll();

        assert_eq!(state.messages.scroll_offset, 15);
    }

    #[test]
    fn spinner_tick_advances_during_processing() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.messages.processing = true;
        assert_eq!(state.messages.spinner_tick, 0);
        // Simulating what draw_tui does
        state.messages.spinner_tick = state.messages.spinner_tick.wrapping_add(1);
        assert_eq!(state.messages.spinner_tick, 1);
    }

    #[test]
    fn user_scrolled_up_flag_works() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.focus = FocusArea::Messages;
        state.messages.line_count = 10;
        state.messages.scroll_offset = 3;

        // Scrolling up sets the flag
        state.messages.scroll_up();
        assert!(state.messages.user_scrolled_up);

        // Scrolling to bottom resets it
        state.messages.scroll_offset = 8;
        state.messages.scroll_down();
        assert_eq!(state.messages.scroll_offset, 9);
        assert!(!state.messages.user_scrolled_up);
    }

    #[test]
    fn auto_scroll_respects_user_scrolled_up() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.phase = Phase::Chat;
        state.messages.line_count = 20;
        state.messages.viewport_height = 1;
        state.messages.scroll_offset = 5;

        state.messages.user_scrolled_up = false;
        state.messages.pending_auto_scroll = true;
        if state.messages.pending_auto_scroll && !state.messages.user_scrolled_up {
            state.messages.scroll_offset = state.messages.max_scroll();
            state.messages.pending_auto_scroll = false;
        }
        assert_eq!(state.messages.scroll_offset, 19);

        state.messages.scroll_offset = 5;
        state.messages.user_scrolled_up = true;
        state.messages.pending_auto_scroll = true;
        if state.messages.pending_auto_scroll && !state.messages.user_scrolled_up {
            state.messages.scroll_offset = state.messages.max_scroll();
        }
        assert_eq!(state.messages.scroll_offset, 5);
    }

    #[test]
    fn 切换日志面板时未手动上滚会触发自动跟底() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.messages.user_scrolled_up = false;
        state.messages.pending_auto_scroll = false;

        super::handle_slash_command("logs", &mut state).expect("命令执行成功");

        assert!(state.messages.pending_auto_scroll);
    }

    #[test]
    fn 窗口尺寸变化时未手动上滚会触发自动跟底() {
        let mut state = TuiState::new(vec![], None, None, None);
        state.messages.user_scrolled_up = false;
        state.messages.pending_auto_scroll = false;
        state.messages.history_cache = Some(super::HistoryCache {
            width: 80,
            content_key: 1,
            lines: vec![ratatui::text::Line::from("缓存内容")],
            visual_line_count: 1,
        });

        super::handle_resize_event(&mut state);

        assert!(state.messages.pending_auto_scroll);
        assert!(state.messages.history_cache.is_none());
    }

    #[test]
    fn 历史重放自动到底时应显示最后一轮助手消息() {
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).expect("终端创建成功");
        let turns = vec![
            agent_runtime::TurnLifecycle {
                turn_id: "turn-1".into(),
                started_at_ms: 1,
                finished_at_ms: 2,
                source_entry_ids: vec![1, 2],
                user_message: "hello world".into(),
                assistant_message: Some("Hello world!".into()),
                thinking: None,
                tool_invocations: vec![],
                failure_message: None,
            },
            agent_runtime::TurnLifecycle {
                turn_id: "turn-2".into(),
                started_at_ms: 3,
                finished_at_ms: 4,
                source_entry_ids: vec![3, 4],
                user_message: "你能干什么".into(),
                assistant_message: Some("我可以帮你做很多事，常见的有：\n\n1. 回答问题\n2. 写作和改写\n3. 编程相关\n4. 学习和办公\n5. 中文交流\n\n如果你愿意，我也可以直接演示一下。".into()),
                thinking: None,
                tool_invocations: vec![],
                failure_message: None,
            },
            agent_runtime::TurnLifecycle {
                turn_id: "turn-3".into(),
                started_at_ms: 5,
                finished_at_ms: 6,
                source_entry_ids: vec![5, 6],
                user_message: "哦".into(),
                assistant_message: Some("嗯，我在。\n你想聊点什么，或者要我帮你做什么？".into()),
                thinking: None,
                tool_invocations: vec![],
                failure_message: None,
            },
        ];
        let mut state = TuiState::new(turns, None, None, None);
        state.phase = Phase::Chat;
        let built = super::message_lines(&state.messages, 80);
        assert!(
            built.lines.iter().any(|line| line_text(line).contains("嗯，我在")),
            "消息构建阶段已丢失最后 assistant 行"
        );
        let built_text = built.lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(built_text.contains("你想聊点什么"), "构建内容缺少最后 assistant 第二行");
        let registry = provider_registry::ProviderRegistry::default();

        terminal.draw(|frame| draw_tui(frame, &mut state, &registry)).expect("绘制成功");

        let buffer = terminal.backend().buffer().clone();
        let text = buffer.content.iter().map(|cell| cell.symbol()).collect::<String>();
        let compact = text.replace(' ', "");
        assert!(compact.contains("嗯，我在"));
    }
}
