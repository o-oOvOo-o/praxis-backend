use super::*;
use praxis_app_core::{
    PraxisChatLocalImageAttachment, PraxisChatMentionBinding, PraxisChatTextElement,
    PraxisChatTextRange, PraxisChatUserMessage, PraxisPendingInputAction,
    PraxisPendingInputActionResult, PraxisPendingSteer as PraxisCorePendingSteer,
    PraxisPendingSteerCompareKey, PraxisThreadInputState,
    merge_user_messages as merge_praxis_user_messages,
};
use praxis_protocol::user_input::ByteRange;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UserMessage {
    pub(super) text: String,
    pub(super) local_images: Vec<LocalImageAttachment>,
    /// Remote image attachments represented as URLs (for example data URLs)
    /// provided by app-gateway clients.
    ///
    /// Unlike `local_images`, these are not created by TUI image attach/paste
    /// flows. The TUI can restore and remove them while editing/backtracking.
    pub(super) remote_image_urls: Vec<String>,
    pub(super) text_elements: Vec<TextElement>,
    pub(super) mention_bindings: Vec<MentionBinding>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct ThreadComposerState {
    pub(super) text: String,
    pub(super) local_images: Vec<LocalImageAttachment>,
    pub(super) remote_image_urls: Vec<String>,
    pub(super) text_elements: Vec<TextElement>,
    pub(super) mention_bindings: Vec<MentionBinding>,
    pub(super) pending_pastes: Vec<(String, String)>,
}

impl ThreadComposerState {
    pub(super) fn has_content(&self) -> bool {
        !self.text.is_empty()
            || !self.local_images.is_empty()
            || !self.remote_image_urls.is_empty()
            || !self.text_elements.is_empty()
            || !self.mention_bindings.is_empty()
            || !self.pending_pastes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ThreadInputState {
    pub(super) composer: Option<ThreadComposerState>,
    pub(super) pending_steers: VecDeque<UserMessage>,
    pub(super) rejected_steers_queue: VecDeque<UserMessage>,
    pub(super) queued_user_messages: VecDeque<UserMessage>,
    pub(super) current_collaboration_mode: CollaborationMode,
    pub(super) active_collaboration_mask: Option<CollaborationModeMask>,
    pub(super) selfwork_plan_path: Option<PathBuf>,
    pub(super) selfwork_last_plan_digest: Option<u64>,
    pub(super) selfwork_stall_count: u8,
    pub(super) selfwork_turn_in_flight: bool,
    pub(super) task_running: bool,
    pub(super) agent_turn_running: bool,
}

impl From<String> for UserMessage {
    fn from(text: String) -> Self {
        Self {
            text,
            local_images: Vec::new(),
            remote_image_urls: Vec::new(),
            // Plain text conversion has no UI element ranges.
            text_elements: Vec::new(),
            mention_bindings: Vec::new(),
        }
    }
}

impl From<&str> for UserMessage {
    fn from(text: &str) -> Self {
        Self {
            text: text.to_string(),
            local_images: Vec::new(),
            remote_image_urls: Vec::new(),
            // Plain text conversion has no UI element ranges.
            text_elements: Vec::new(),
            mention_bindings: Vec::new(),
        }
    }
}

pub(super) struct PendingSteer {
    pub(super) user_message: UserMessage,
    pub(super) compare_key: PendingSteerCompareKey,
}

pub(crate) fn create_initial_user_message(
    text: Option<String>,
    local_image_paths: Vec<PathBuf>,
    text_elements: Vec<TextElement>,
) -> Option<UserMessage> {
    let text = text.unwrap_or_default();
    if text.is_empty() && local_image_paths.is_empty() {
        None
    } else {
        let local_images = local_image_paths
            .into_iter()
            .enumerate()
            .map(|(idx, path)| LocalImageAttachment {
                placeholder: local_image_label_text(idx + 1),
                path,
            })
            .collect();
        Some(UserMessage {
            text,
            local_images,
            remote_image_urls: Vec::new(),
            text_elements,
            mention_bindings: Vec::new(),
        })
    }
}

pub(super) fn merge_user_messages(messages: Vec<UserMessage>) -> UserMessage {
    UserMessage::from(merge_praxis_user_messages(
        messages.into_iter().map(PraxisChatUserMessage::from),
    ))
}

impl From<UserMessage> for PraxisChatUserMessage {
    fn from(message: UserMessage) -> Self {
        Self {
            text: message.text,
            local_images: message
                .local_images
                .into_iter()
                .map(|attachment| PraxisChatLocalImageAttachment {
                    placeholder: attachment.placeholder,
                    path: attachment.path,
                })
                .collect(),
            remote_image_urls: message.remote_image_urls,
            text_elements: message
                .text_elements
                .into_iter()
                .map(|element| PraxisChatTextElement {
                    byte_range: PraxisChatTextRange {
                        start: element.byte_range.start,
                        end: element.byte_range.end,
                    },
                    placeholder: element
                        ._placeholder_for_conversion_only()
                        .map(str::to_owned),
                })
                .collect(),
            mention_bindings: message
                .mention_bindings
                .into_iter()
                .map(|binding| PraxisChatMentionBinding {
                    mention: binding.mention,
                    path: binding.path,
                })
                .collect(),
            context_items: Vec::new(),
        }
    }
}

impl From<PraxisChatUserMessage> for UserMessage {
    fn from(message: PraxisChatUserMessage) -> Self {
        Self {
            text: message.text,
            local_images: message
                .local_images
                .into_iter()
                .map(|attachment| LocalImageAttachment {
                    placeholder: attachment.placeholder,
                    path: attachment.path,
                })
                .collect(),
            remote_image_urls: message.remote_image_urls,
            text_elements: message
                .text_elements
                .into_iter()
                .map(|element| {
                    TextElement::new(
                        ByteRange {
                            start: element.byte_range.start,
                            end: element.byte_range.end,
                        },
                        element.placeholder,
                    )
                })
                .collect(),
            mention_bindings: message
                .mention_bindings
                .into_iter()
                .map(|binding| MentionBinding {
                    mention: binding.mention,
                    path: binding.path,
                })
                .collect(),
        }
    }
}

impl ChatWidget {
    pub(super) fn praxis_thread_input_state(&self) -> PraxisThreadInputState {
        PraxisThreadInputState {
            pending_steers: self
                .pending_steers
                .iter()
                .map(|steer| PraxisCorePendingSteer {
                    user_message: PraxisChatUserMessage::from(steer.user_message.clone()),
                    compare_key: PraxisPendingSteerCompareKey {
                        message: steer.compare_key.message.clone(),
                        image_count: steer.compare_key.image_count,
                    },
                })
                .collect(),
            rejected_steers_queue: self
                .rejected_steers_queue
                .iter()
                .cloned()
                .map(PraxisChatUserMessage::from)
                .collect(),
            queued_user_messages: self
                .queued_user_messages
                .iter()
                .cloned()
                .map(PraxisChatUserMessage::from)
                .collect(),
            task_running: self.bottom_pane.is_task_running(),
            agent_turn_running: self.agent_turn_running,
            submit_pending_steers_after_interrupt: self.submit_pending_steers_after_interrupt,
            ..PraxisThreadInputState::default()
        }
    }

    pub(super) fn request_pending_steer_interrupt(&mut self) -> bool {
        let mut input_state = self.praxis_thread_input_state();
        let requested = matches!(
            input_state.apply_pending_input_action(
                PraxisPendingInputAction::InterruptAndSubmitPendingSteers,
            ),
            PraxisPendingInputActionResult::InterruptRequested
        );
        self.submit_pending_steers_after_interrupt =
            input_state.submit_pending_steers_after_interrupt;
        requested
    }

    pub(super) fn rollback_pending_steer_interrupt(&mut self) {
        self.submit_pending_steers_after_interrupt = false;
    }
}
