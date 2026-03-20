use agent_core::{CompletionRequest, LanguageModel, ToolCall, ToolExecutor, ToolResult};

use crate::{
    AgentStartEvent, BeforeAgentStartEvent, BeforeProviderRequestEvent, InputEvent, ToolCallEvent,
    ToolInvocationOutcome, ToolResultEvent, TurnEndEvent, TurnLifecycle, TurnStartEvent,
};

use super::{AgentRuntime, RuntimeError};

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    pub(super) fn ensure_agent_started(&mut self) -> Result<(), RuntimeError> {
        if self.agent_started {
            return Ok(());
        }

        let mut event = BeforeAgentStartEvent {
            session_id: self.session_id.clone(),
            model: self.model_identity.clone(),
            instructions: self.instructions.clone(),
            user_agent: self.user_agent.clone(),
            workspace_root: self.workspace_root.clone(),
            disabled_tools: self.disabled_tools.clone(),
            prompt_cache: self.prompt_cache.clone(),
            request_timeout: self.request_timeout.clone(),
            max_tool_calls_per_turn: self.max_tool_calls_per_turn,
            context_pressure_threshold: self.context_pressure_threshold,
        };

        for handler in &self.hooks.before_agent_start {
            handler(&mut event)?;
        }

        self.instructions = normalize_text(event.instructions);
        self.user_agent = normalize_text(event.user_agent);
        self.workspace_root = event.workspace_root;
        self.disabled_tools = event.disabled_tools;
        self.prompt_cache = event.prompt_cache;
        self.request_timeout = event.request_timeout;
        self.max_tool_calls_per_turn = event.max_tool_calls_per_turn.max(1);
        self.context_pressure_threshold = event.context_pressure_threshold.clamp(0.0, 1.0);
        self.agent_started = true;

        let event = AgentStartEvent {
            session_id: self.session_id.clone(),
            model: self.model_identity.clone(),
            instructions: self.instructions.clone(),
            visible_tools: self.visible_tools(),
        };
        for handler in &self.hooks.agent_start {
            handler(&event);
        }

        Ok(())
    }

    pub(super) fn rewrite_input(
        &self,
        user_input: impl Into<String>,
    ) -> Result<String, RuntimeError> {
        let mut event =
            InputEvent { session_id: self.session_id.clone(), input: user_input.into() };
        for handler in &self.hooks.input {
            handler(&mut event)?;
        }
        Ok(event.input)
    }

    pub(super) fn notify_turn_start(&self, turn_id: &str, user_message: &str) {
        let event = TurnStartEvent {
            session_id: self.session_id.clone(),
            turn_id: turn_id.to_string(),
            user_message: user_message.to_string(),
        };
        for handler in &self.hooks.turn_start {
            handler(&event);
        }
    }

    pub(super) fn prepare_request_with_hooks(
        &mut self,
        turn_id: &str,
        request_kind: &str,
        step_index: u32,
        request: CompletionRequest,
    ) -> Result<CompletionRequest, RuntimeError> {
        self.ensure_agent_started()?;

        let mut event = BeforeProviderRequestEvent {
            session_id: self.session_id.clone(),
            turn_id: turn_id.to_string(),
            request_kind: request_kind.to_string(),
            step_index,
            request,
        };
        for handler in &self.hooks.before_provider_request {
            handler(&mut event)?;
        }
        event.request.instructions = normalize_text(event.request.instructions);
        Ok(event.request)
    }

    pub(super) fn resolve_tool_call_override(
        &self,
        turn_id: &str,
        call: &ToolCall,
    ) -> Result<Option<ToolResult>, RuntimeError> {
        let mut event = ToolCallEvent {
            session_id: self.session_id.clone(),
            turn_id: turn_id.to_string(),
            call: call.clone(),
            override_result: None,
        };
        for handler in &self.hooks.tool_call {
            handler(&mut event)?;
        }

        if let Some(result) = event.override_result.as_ref()
            && (result.invocation_id != call.invocation_id || result.tool_name != call.tool_name)
        {
            return Err(RuntimeError::tool_result_mismatch(call, result));
        }

        Ok(event.override_result)
    }

    pub(super) fn rewrite_tool_outcome(
        &self,
        turn_id: &str,
        call: &ToolCall,
        outcome: ToolInvocationOutcome,
    ) -> Result<ToolInvocationOutcome, RuntimeError> {
        let mut event = ToolResultEvent {
            session_id: self.session_id.clone(),
            turn_id: turn_id.to_string(),
            call: call.clone(),
            outcome,
        };
        for handler in &self.hooks.tool_result {
            handler(&mut event)?;
        }

        if let ToolInvocationOutcome::Succeeded { result } = &event.outcome
            && (result.invocation_id != call.invocation_id || result.tool_name != call.tool_name)
        {
            return Err(RuntimeError::tool_result_mismatch(call, result));
        }

        Ok(event.outcome)
    }

    pub(super) fn notify_turn_end(&self, turn: &TurnLifecycle) {
        let event = TurnEndEvent { session_id: self.session_id.clone(), turn: turn.clone() };
        for handler in &self.hooks.turn_end {
            handler(&event);
        }
    }
}

fn normalize_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| (!text.trim().is_empty()).then_some(text))
}
