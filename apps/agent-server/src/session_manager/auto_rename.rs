use std::sync::Arc;

use agent_core::{AbortSignal, CompletionRequest, LanguageModel, LlmTraceRequestContext};
use agent_runtime::TurnLifecycle;
use agent_store::{AiaStore, SessionAutoRenamePolicy, SessionRecord, SessionTitleSource};
use provider_registry::ProviderRegistry;

use crate::{
    model::{ProviderLaunchChoice, build_model_from_selection, model_identity_from_selection},
    sse::SsePayload,
};

use super::load_session_tape_with_repair;

pub(super) struct SessionAutoRenameService {
    pub(super) store: Arc<AiaStore>,
    pub(super) registry: ProviderRegistry,
    pub(super) broadcast_tx: tokio::sync::broadcast::Sender<SsePayload>,
    pub(super) sessions_dir: std::path::PathBuf,
    pub(super) user_agent: String,
}

impl SessionAutoRenameService {
    pub(super) async fn maybe_schedule_after_turn(
        &self,
        session_id: &str,
        turn: &TurnLifecycle,
        allow_schedule: bool,
    ) {
        if !should_count_turn_for_auto_rename(turn) {
            return;
        }

        let Ok(Some(record)) = self
            .store
            .note_completed_user_turn_for_auto_rename_async(session_id.to_string(), allow_schedule)
            .await
        else {
            return;
        };

        let store = self.store.clone();
        let registry = self.registry.clone();
        let broadcast_tx = self.broadcast_tx.clone();
        let sessions_dir = self.sessions_dir.clone();
        let user_agent = self.user_agent.clone();
        let session_id = session_id.to_string();
        tokio::spawn(async move {
            let service = SessionAutoRenameService {
                store,
                registry,
                broadcast_tx,
                sessions_dir,
                user_agent,
            };
            let _ = service.run_once(&session_id, &record).await;
        });
    }

    async fn run_once(&self, session_id: &str, record: &SessionRecord) -> Result<(), String> {
        if !should_schedule_session_rename(record) {
            return Ok(());
        }

        let session_path = self.sessions_dir.join(format!("{session_id}.jsonl"));
        let tape = load_session_tape_with_repair(&session_path).map_err(|error| error.message)?;
        let recent_turns = latest_user_turns_from_tape(&tape, 5);
        if recent_turns.len() < 3 {
            return Ok(());
        }

        let title = self.generate_title(record, &recent_turns).await?;
        let title = title.trim().to_string();
        if title.is_empty() {
            return Ok(());
        }

        let updated = self
            .store
            .apply_auto_rename_title_async(session_id.to_string(), title)
            .await
            .map_err(|error| error.to_string())?;

        let Some(updated) = updated else {
            return Ok(());
        };

        let model = projected_session_model_for_record(&self.registry, &updated, &session_path)
            .unwrap_or_else(|| updated.model.clone());
        let _ = self.broadcast_tx.send(SsePayload::SessionUpdated {
            session_id: updated.id,
            title: updated.title,
            title_source: updated.title_source,
            auto_rename_policy: updated.auto_rename_policy,
            updated_at: updated.updated_at,
            last_active_at: updated.last_active_at,
            model,
        });

        Ok(())
    }

    async fn generate_title(
        &self,
        record: &SessionRecord,
        recent_user_turns: &[String],
    ) -> Result<String, String> {
        let selection = resolve_selection_for_session(&self.registry, record);
        let (identity, model) = build_model_from_selection(selection, Some(self.store.clone()))
            .map_err(|error| error.to_string())?;

        let prompt = agent_prompts::render_title_generator_prompt(
            agent_prompts::TitleGeneratorPromptContext {
                current_title: record.title.clone(),
                title_source: serialize_title_source(record.title_source),
                recent_user_turns: recent_user_turns.to_vec(),
            },
        );
        let request = CompletionRequest {
            model: identity,
            instructions: Some(prompt),
            conversation: vec![],
            max_output_tokens: Some(32),
            available_tools: vec![],
            parallel_tool_calls: Some(false),
            prompt_cache: None,
            user_agent: Some(self.user_agent.clone()),
            timeout: None,
            trace_context: Some(build_rename_trace_context(record)),
        };

        let completion = model
            .complete_streaming(request, &AbortSignal::new(), &mut |_| {})
            .await
            .map_err(|error| error.to_string())?;
        Ok(completion.plain_text())
    }
}

fn resolve_selection_for_session(
    registry: &ProviderRegistry,
    record: &SessionRecord,
) -> ProviderLaunchChoice {
    registry
        .providers()
        .iter()
        .find(|provider| provider.has_model(&record.model))
        .and_then(|provider| {
            registry.resolve_model(&agent_core::ModelRef::new(&provider.id, &record.model)).ok()
        })
        .map(|spec| ProviderLaunchChoice::Resolved { spec, reasoning_effort: None })
        .unwrap_or(ProviderLaunchChoice::Bootstrap)
}

fn build_rename_trace_context(record: &SessionRecord) -> LlmTraceRequestContext {
    let run_id = format!("{}-session-rename", record.id);
    let trace_id = aia_config::build_trace_id(&run_id);
    let root_span_id = aia_config::build_root_span_id(&run_id);
    let span_id = aia_config::build_request_span_id(&run_id, "session_rename", 0);
    LlmTraceRequestContext {
        session_id: Some(record.id.clone()),
        trace_id,
        span_id,
        parent_span_id: Some(root_span_id.clone()),
        root_span_id,
        operation_name: "chat".to_string(),
        turn_id: format!("{}:session-rename", record.id),
        run_id,
        request_kind: "session_rename".to_string(),
        step_index: 0,
    }
}

fn serialize_title_source(title_source: SessionTitleSource) -> String {
    match title_source {
        SessionTitleSource::Default => "default",
        SessionTitleSource::Auto => "auto",
        SessionTitleSource::Manual => "manual",
        SessionTitleSource::Channel => "channel",
    }
    .to_string()
}

pub(super) fn should_count_turn_for_auto_rename(turn: &TurnLifecycle) -> bool {
    matches!(turn.outcome, agent_runtime::TurnOutcome::Succeeded)
        && turn.user_messages.iter().any(|m| !m.trim().is_empty())
}

pub(super) fn should_schedule_session_rename(record: &SessionRecord) -> bool {
    record.auto_rename_policy == SessionAutoRenamePolicy::Enabled
        && matches!(record.title_source, SessionTitleSource::Default | SessionTitleSource::Auto)
}

fn latest_user_turns_from_tape(tape: &session_tape::SessionTape, limit: usize) -> Vec<String> {
    let snapshots = crate::runtime_worker::rebuild_session_snapshots_from_tape(tape);
    snapshots
        .history
        .into_iter()
        .rev()
        .filter(|turn| should_count_turn_for_auto_rename(turn))
        .map(|turn| turn.user_messages.join("\n"))
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn projected_session_model_for_record(
    registry: &ProviderRegistry,
    record: &SessionRecord,
    session_path: &std::path::Path,
) -> Option<String> {
    let _ = session_path;
    let selection = resolve_selection_for_session(registry, record);
    Some(model_identity_from_selection(&selection).name)
}
