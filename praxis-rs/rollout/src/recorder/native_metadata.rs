use praxis_protocol::ThreadId;
use praxis_protocol::protocol::RolloutItem;
use praxis_thread_store::ThreadSummary;
use praxis_thread_store_contracts::ContentRef;
use praxis_thread_store_contracts::ThreadResumeConfig;

pub(super) const METADATA_GENERATION: u32 = 1;
const PREVIEW_CHAR_LIMIT: usize = 160;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct NativeMetadataDelta {
    pub workspace: bool,
    pub preview: bool,
    pub resume_config: bool,
    pub dynamic_tools: bool,
}

impl NativeMetadataDelta {
    pub fn merge(&mut self, other: Self) {
        self.workspace |= other.workspace;
        self.preview |= other.preview;
        self.resume_config |= other.resume_config;
        self.dynamic_tools |= other.dynamic_tools;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeRolloutMetadata {
    pub workspace: Option<String>,
    pub preview: Option<String>,
    pub first_user_message: Option<String>,
    pub resume_config: ThreadResumeConfig,
    pub dynamic_tools: Option<ContentRef>,
}

impl Default for NativeRolloutMetadata {
    fn default() -> Self {
        Self {
            workspace: None,
            preview: None,
            first_user_message: None,
            resume_config: ThreadResumeConfig {
                model: None,
                model_provider: None,
                reasoning_effort: None,
            },
            dynamic_tools: None,
        }
    }
}

impl NativeRolloutMetadata {
    pub fn from_summary(summary: &ThreadSummary) -> Self {
        Self {
            workspace: Some(summary.workspace.clone()),
            preview: summary.preview.clone(),
            first_user_message: summary.first_user_message.clone(),
            resume_config: ThreadResumeConfig {
                model: summary.model.clone(),
                model_provider: summary.model_provider.clone(),
                reasoning_effort: summary.reasoning_effort.clone(),
            },
            dynamic_tools: None,
        }
    }

    pub fn apply(
        &mut self,
        expected_thread_id: ThreadId,
        item: &RolloutItem,
    ) -> NativeMetadataDelta {
        match item {
            RolloutItem::SessionMeta(meta) if meta.meta.id == expected_thread_id => {
                let mut delta = NativeMetadataDelta::default();
                if !meta.meta.cwd.as_os_str().is_empty() {
                    delta.workspace |= replace_option(
                        &mut self.workspace,
                        Some(meta.meta.cwd.to_string_lossy().into_owned()),
                    );
                }
                if let Some(provider) = meta.meta.model_provider.as_ref() {
                    delta.resume_config |= replace_option(
                        &mut self.resume_config.model_provider,
                        Some(provider.clone()),
                    );
                }
                if let Some(tools) = meta.meta.dynamic_tools.as_ref()
                    && let Ok(text) = serde_json::to_string(tools)
                {
                    delta.dynamic_tools |= replace_option(
                        &mut self.dynamic_tools,
                        Some(ContentRef::InlineText { text }),
                    );
                }
                delta
            }
            RolloutItem::TurnContext(context) => {
                let mut delta = NativeMetadataDelta::default();
                if self.workspace.is_none() && !context.cwd.as_os_str().is_empty() {
                    self.workspace = Some(context.cwd.to_string_lossy().into_owned());
                    delta.workspace = true;
                }
                delta.resume_config |=
                    replace_option(&mut self.resume_config.model, Some(context.model.clone()));
                let reasoning_effort = context
                    .effort
                    .as_ref()
                    .and_then(|effort| serde_json::to_value(effort).ok())
                    .and_then(|value| value.as_str().map(str::to_owned));
                delta.resume_config |=
                    replace_option(&mut self.resume_config.reasoning_effort, reasoning_effort);
                delta
            }
            _ => {
                if let Some(preview) = praxis_state::thread_preview::rollout_item_preview(item) {
                    let preview = truncate_preview(preview.as_display_text());
                    let mut changed = false;
                    if self.first_user_message.is_none() {
                        self.first_user_message = Some(preview.clone());
                        changed = true;
                    }
                    changed |= replace_option(&mut self.preview, Some(preview));
                    NativeMetadataDelta {
                        preview: changed,
                        ..NativeMetadataDelta::default()
                    }
                } else {
                    NativeMetadataDelta::default()
                }
            }
        }
    }
}

fn replace_option<T: Eq>(target: &mut Option<T>, value: Option<T>) -> bool {
    if *target == value {
        return false;
    }
    *target = value;
    true
}

fn truncate_preview(text: &str) -> String {
    text.char_indices()
        .nth(PREVIEW_CHAR_LIMIT)
        .map_or(text, |(end, _)| &text[..end])
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::NativeRolloutMetadata;
    use praxis_protocol::ThreadId;
    use praxis_protocol::protocol::EventMsg;
    use praxis_protocol::protocol::RolloutItem;
    use praxis_protocol::protocol::UserMessageEvent;

    #[test]
    fn preview_projection_is_bounded_and_keeps_the_first_user_message() {
        let thread_id = ThreadId::new();
        let mut metadata = NativeRolloutMetadata::default();
        for message in ["界".repeat(161), "later".to_string()] {
            assert!(
                metadata
                    .apply(
                        thread_id,
                        &RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
                            message,
                            images: None,
                            local_images: Vec::new(),
                            text_elements: Vec::new(),
                        })),
                    )
                    .preview
            );
        }

        assert_eq!(metadata.preview.as_deref(), Some("later"));
        assert_eq!(
            metadata
                .first_user_message
                .as_deref()
                .expect("first preview")
                .chars()
                .count(),
            160
        );
    }
}
