use agent_core::{
    AbortSignal, Completion, CompletionRequest, CoreError, LanguageModel, StreamEvent,
};
use async_trait::async_trait;

pub(super) struct BootstrapModel;

#[async_trait]
impl LanguageModel for BootstrapModel {
    type Error = CoreError;

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        _abort: &AbortSignal,
        sink: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<Completion, Self::Error> {
        let latest_user = request
            .conversation
            .iter()
            .rev()
            .find_map(|item| {
                item.as_message()
                    .filter(|message| message.role == agent_core::Role::User)
                    .map(|message| message.content.clone())
            })
            .unwrap_or_else(|| "空输入".into());

        let completion = Completion::text(format!(
            "Bootstrap 模式收到：{latest_user}。请配置真实 provider 以使用完整功能。"
        ));
        sink(StreamEvent::Done);
        Ok(completion)
    }
}
