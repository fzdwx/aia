use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use provider_registry::ProviderKind;
use session_tape::SessionProviderBinding;

use crate::state::SharedState;
use agent_core::ReasoningEffort;

use super::{
    AutoCompressRequest, CreateSessionRequest, HandoffRequest, PendingQuestionResponse,
    ResolvePendingQuestionRequest, SendMessageRequest, SessionInfoResponse, SessionQuery,
    SessionSettingsResponse, UpdateSessionSettingsRequest, WidgetClientEventRequest,
};
use crate::routes::common::{
    JsonResponse, json_response, require_session_id, resolve_session_id,
    runtime_worker_error_response, session_resolution_error_response,
};

pub(crate) async fn list_sessions(State(state): State<SharedState>) -> JsonResponse {
    match state.session_manager.list_sessions().await {
        Ok(sessions) => json_response(StatusCode::OK, sessions),
        Err(error) => (error.status, Json(serde_json::json!({ "error": error.message }))),
    }
}

pub(crate) async fn create_session(
    State(state): State<SharedState>,
    Json(body): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    match state.session_manager.create_session(body.title).await {
        Ok(record) => json_response(StatusCode::CREATED, record),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn delete_session(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.session_manager.delete_session(id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn get_session_info(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), query.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.get_session_info(session_id).await {
        Ok(stats) => json_response(
            StatusCode::OK,
            SessionInfoResponse {
                stats,
                workspace_root: state.session_manager.workspace_root().display().to_string(),
            },
        ),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn get_session_settings(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), query.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.get_session_settings(session_id).await {
        Ok(settings) => {
            json_response(StatusCode::OK, SessionSettingsResponse::from_binding(settings))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn get_pending_question(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), query.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.get_pending_question(session_id).await {
        Ok(request) => json_response(
            StatusCode::OK,
            PendingQuestionResponse { pending: request.is_some(), request },
        ),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn resolve_pending_question(
    State(state): State<SharedState>,
    Json(body): Json<ResolvePendingQuestionRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.resolve_pending_question(session_id, body.result).await {
        Ok(()) => json_response(StatusCode::OK, serde_json::json!({ "ok": true })),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn cancel_pending_question(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), query.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.cancel_pending_question(session_id).await {
        Ok(()) => json_response(StatusCode::OK, serde_json::json!({ "ok": true })),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn update_session_settings(
    State(state): State<SharedState>,
    Json(body): Json<UpdateSessionSettingsRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    let Some(provider) = body.provider else {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "error": "provider is required" }),
        );
    };
    let Some(model) = body.model else {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "error": "model is required" }),
        );
    };

    let binding = {
        let registry = crate::session_manager::read_lock(&state.provider_registry_snapshot);
        let Some(profile) =
            registry.providers().iter().find(|candidate| candidate.name == provider)
        else {
            return json_response(
                StatusCode::NOT_FOUND,
                serde_json::json!({ "error": format!("provider 不存在：{provider}") }),
            );
        };

        let Some(selected_model) = profile.models.iter().find(|candidate| candidate.id == model)
        else {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({ "error": format!("模型不存在：{model}") }),
            );
        };

        let reasoning_effort =
            match ReasoningEffort::parse_optional(body.reasoning_effort.as_deref()) {
                Ok(reasoning_effort) => reasoning_effort,
                Err(error) => {
                    return json_response(
                        StatusCode::BAD_REQUEST,
                        serde_json::json!({ "error": error }),
                    );
                }
            };

        let protocol = match profile.kind {
            ProviderKind::OpenAiResponses => "openai-responses",
            ProviderKind::OpenAiChatCompletions => "openai-chat-completions",
        }
        .to_string();

        SessionProviderBinding::Provider {
            name: profile.name.clone(),
            model: model.clone(),
            base_url: profile.base_url.clone(),
            protocol,
            reasoning_effort: ReasoningEffort::serialize_optional(reasoning_effort)
                .filter(|_| selected_model.supports_reasoning),
        }
    };

    match state.session_manager.update_session_settings(session_id.clone(), binding).await {
        Ok(info) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "name": info.name,
                "model": info.model,
                "connected": info.connected,
            }),
        ),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn create_handoff(
    State(state): State<SharedState>,
    Json(body): Json<HandoffRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.create_handoff(session_id, body.name, body.summary).await {
        Ok(anchor_entry_id) => {
            (StatusCode::OK, Json(serde_json::json!({ "anchor_entry_id": anchor_entry_id })))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn auto_compress_session(
    State(state): State<SharedState>,
    Json(body): Json<AutoCompressRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.auto_compress_session(session_id).await {
        Ok(compressed) => (StatusCode::OK, Json(serde_json::json!({ "compressed": compressed }))),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn get_history(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> JsonResponse {
    let session_id = match resolve_session_id(state.as_ref(), query.session_id).await {
        Ok(Some(session_id)) => session_id,
        Ok(None) => return json_response(StatusCode::OK, Vec::<()>::new()),
        Err(error) => return session_resolution_error_response(error),
    };

    match state.session_manager.get_history(session_id).await {
        Ok(turns) => {
            let limit = query.limit.unwrap_or(5).clamp(1, 200);
            let end_exclusive = query
                .before_turn_id
                .as_ref()
                .and_then(|turn_id| turns.iter().position(|turn| &turn.turn_id == turn_id))
                .unwrap_or(turns.len());
            let start = end_exclusive.saturating_sub(limit);
            let page = turns[start..end_exclusive].to_vec();
            let has_more = start > 0;
            let next_before_turn_id = page.first().map(|turn| turn.turn_id.clone());

            json_response(
                StatusCode::OK,
                serde_json::json!({
                    "turns": page,
                    "has_more": has_more,
                    "next_before_turn_id": next_before_turn_id,
                }),
            )
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn get_current_turn(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> JsonResponse {
    let session_id = match resolve_session_id(state.as_ref(), query.session_id).await {
        Ok(Some(session_id)) => session_id,
        Ok(None) => return json_response(StatusCode::OK, Option::<()>::None),
        Err(error) => return session_resolution_error_response(error),
    };

    match state.session_manager.get_current_turn(session_id).await {
        Ok(current) => json_response(StatusCode::OK, current),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn send_message(
    State(state): State<SharedState>,
    Json(body): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.queue_message(session_id, body.message).await {
        Ok(response) => {
            let status = match response.status {
                crate::session_manager::QueueMessageStatus::Started => StatusCode::ACCEPTED,
                crate::session_manager::QueueMessageStatus::Queued => StatusCode::OK,
            };
            (status, Json(serde_json::to_value(response).unwrap()))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn get_queue(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), query.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.get_queue(session_id).await {
        Ok(messages) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "messages": messages,
            }),
        ),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn delete_queued_message(
    State(state): State<SharedState>,
    Path(message_id): Path<String>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), query.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.delete_queued_message(session_id, message_id.clone()).await {
        Ok(()) => json_response(
            StatusCode::OK,
            serde_json::json!({ "deleted": true, "message_id": message_id }),
        ),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn report_widget_client_event(
    State(state): State<SharedState>,
    Json(body): Json<WidgetClientEventRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    if body.invocation_id.trim().is_empty() {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "error": "invocation_id is required" }),
        );
    }

    match state
        .session_manager
        .report_widget_client_event(session_id, body.turn_id, body.invocation_id, body.event)
        .await
    {
        Ok(()) => json_response(StatusCode::OK, serde_json::json!({ "ok": true })),
        Err(error) => runtime_worker_error_response(error),
    }
}

pub(crate) async fn interrupt_turn(
    State(state): State<SharedState>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), query.session_id).await {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };

    match state.session_manager.interrupt_turn(session_id).await {
        Ok(interrupted) => {
            json_response(StatusCode::OK, serde_json::json!({ "interrupted": interrupted }))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}
