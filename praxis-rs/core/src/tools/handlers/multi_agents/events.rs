use crate::agent::AgentStatus;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use praxis_protocol::ThreadId;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::protocol::CollabAgentInteractionBeginEvent;
use praxis_protocol::protocol::CollabAgentInteractionEndEvent;
use praxis_protocol::protocol::CollabAgentInteractionKind;
use praxis_protocol::protocol::CollabAgentRef;
use praxis_protocol::protocol::CollabAgentSpawnBeginEvent;
use praxis_protocol::protocol::CollabAgentSpawnEndEvent;
use praxis_protocol::protocol::CollabAgentStatusEntry;
use praxis_protocol::protocol::CollabCloseBeginEvent;
use praxis_protocol::protocol::CollabCloseEndEvent;
use praxis_protocol::protocol::CollabWaitingBeginEvent;
use praxis_protocol::protocol::CollabWaitingEndEvent;
use praxis_protocol::protocol::EventMsg;
use std::collections::HashMap;

pub(super) struct CollabAgentEventEmitter<'a> {
    session: &'a Session,
    turn: &'a TurnContext,
    call_id: &'a str,
}

impl<'a> CollabAgentEventEmitter<'a> {
    pub(super) fn new(session: &'a Session, turn: &'a TurnContext, call_id: &'a str) -> Self {
        Self {
            session,
            turn,
            call_id,
        }
    }

    pub(super) async fn spawn_begin(
        &self,
        prompt: String,
        model: String,
        reasoning_effort: ReasoningEffort,
    ) {
        self.send(CollabAgentSpawnBeginEvent {
            call_id: self.call_id.to_string(),
            sender_thread_id: self.sender_thread_id(),
            prompt,
            model,
            reasoning_effort,
        })
        .await;
    }

    pub(super) async fn spawn_end(&self, input: CollabSpawnEndEventInput) {
        self.send(CollabAgentSpawnEndEvent {
            call_id: self.call_id.to_string(),
            sender_thread_id: self.sender_thread_id(),
            new_thread_id: input.new_thread_id,
            new_agent_base_name: input.new_agent_base_name,
            new_agent_title: input.new_agent_title,
            new_agent_display_name: input.new_agent_display_name,
            new_agent_role: input.new_agent_role,
            prompt: input.prompt,
            model: input.model,
            reasoning_effort: input.reasoning_effort,
            status: input.status,
        })
        .await;
    }

    pub(super) async fn interaction_begin(
        &self,
        receiver_thread_id: ThreadId,
        kind: CollabAgentInteractionKind,
        prompt: String,
    ) {
        self.send(CollabAgentInteractionBeginEvent {
            call_id: self.call_id.to_string(),
            sender_thread_id: self.sender_thread_id(),
            receiver_thread_id,
            kind,
            prompt,
        })
        .await;
    }

    pub(super) async fn interaction_end(&self, input: CollabInteractionEndEventInput) {
        self.send(CollabAgentInteractionEndEvent {
            call_id: self.call_id.to_string(),
            sender_thread_id: self.sender_thread_id(),
            receiver_thread_id: input.receiver_thread_id,
            kind: input.kind,
            receiver_agent_base_name: input.receiver_agent_base_name,
            receiver_agent_title: input.receiver_agent_title,
            receiver_agent_display_name: input.receiver_agent_display_name,
            receiver_agent_role: input.receiver_agent_role,
            prompt: input.prompt,
            status: input.status,
        })
        .await;
    }

    pub(super) async fn waiting_begin(
        &self,
        receiver_thread_ids: Vec<ThreadId>,
        receiver_agents: Vec<CollabAgentRef>,
    ) {
        self.send(CollabWaitingBeginEvent {
            sender_thread_id: self.sender_thread_id(),
            receiver_thread_ids,
            receiver_agents,
            call_id: self.call_id.to_string(),
        })
        .await;
    }

    pub(super) async fn waiting_end(
        &self,
        agent_statuses: Vec<CollabAgentStatusEntry>,
        statuses: HashMap<ThreadId, AgentStatus>,
    ) {
        self.send(CollabWaitingEndEvent {
            sender_thread_id: self.sender_thread_id(),
            call_id: self.call_id.to_string(),
            agent_statuses,
            statuses,
        })
        .await;
    }

    pub(super) async fn close_begin(&self, receiver_thread_id: ThreadId) {
        self.send(CollabCloseBeginEvent {
            call_id: self.call_id.to_string(),
            sender_thread_id: self.sender_thread_id(),
            receiver_thread_id,
        })
        .await;
    }

    pub(super) async fn close_end(&self, input: CollabCloseEndEventInput) {
        self.send(CollabCloseEndEvent {
            call_id: self.call_id.to_string(),
            sender_thread_id: self.sender_thread_id(),
            receiver_thread_id: input.receiver_thread_id,
            receiver_agent_base_name: input.receiver_agent_base_name,
            receiver_agent_title: input.receiver_agent_title,
            receiver_agent_display_name: input.receiver_agent_display_name,
            receiver_agent_role: input.receiver_agent_role,
            status: input.status,
        })
        .await;
    }

    async fn send(&self, msg: impl Into<EventMsg>) {
        self.session.send_event(self.turn, msg.into()).await;
    }

    fn sender_thread_id(&self) -> ThreadId {
        self.session.conversation_id
    }
}

pub(super) struct CollabSpawnEndEventInput {
    pub(super) new_thread_id: Option<ThreadId>,
    pub(super) new_agent_base_name: Option<String>,
    pub(super) new_agent_title: Option<String>,
    pub(super) new_agent_display_name: Option<String>,
    pub(super) new_agent_role: Option<String>,
    pub(super) prompt: String,
    pub(super) model: String,
    pub(super) reasoning_effort: ReasoningEffort,
    pub(super) status: AgentStatus,
}

pub(super) struct CollabInteractionEndEventInput {
    pub(super) receiver_thread_id: ThreadId,
    pub(super) kind: CollabAgentInteractionKind,
    pub(super) receiver_agent_base_name: Option<String>,
    pub(super) receiver_agent_title: Option<String>,
    pub(super) receiver_agent_display_name: Option<String>,
    pub(super) receiver_agent_role: Option<String>,
    pub(super) prompt: String,
    pub(super) status: AgentStatus,
}

pub(super) struct CollabCloseEndEventInput {
    pub(super) receiver_thread_id: ThreadId,
    pub(super) receiver_agent_base_name: Option<String>,
    pub(super) receiver_agent_title: Option<String>,
    pub(super) receiver_agent_display_name: Option<String>,
    pub(super) receiver_agent_role: Option<String>,
    pub(super) status: AgentStatus,
}
