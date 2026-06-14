use praxis_app_gateway_protocol::CollabAgentState as ApiCollabAgentStatus;
use praxis_app_gateway_protocol::CollabAgentTool;
use praxis_app_gateway_protocol::CollabAgentToolCallStatus as ApiCollabToolCallStatus;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_protocol::protocol::AgentStatus;
use praxis_protocol::protocol::CollabAgentInteractionBeginEvent;
use praxis_protocol::protocol::CollabAgentInteractionEndEvent;
use praxis_protocol::protocol::CollabAgentInteractionKind;
use praxis_protocol::protocol::CollabAgentSpawnBeginEvent;
use praxis_protocol::protocol::CollabAgentSpawnEndEvent;
use praxis_protocol::protocol::CollabCloseBeginEvent;
use praxis_protocol::protocol::CollabCloseEndEvent;
use praxis_protocol::protocol::CollabResumeBeginEvent;
use praxis_protocol::protocol::CollabResumeEndEvent;
use praxis_protocol::protocol::CollabWaitingBeginEvent;
use praxis_protocol::protocol::CollabWaitingEndEvent;
use std::collections::HashMap;

pub(crate) fn collab_agent_status_failed(status: &AgentStatus) -> bool {
    matches!(status, AgentStatus::Errored(_) | AgentStatus::NotFound)
}

pub(crate) fn collab_spawn_begin_item(event: CollabAgentSpawnBeginEvent) -> ThreadItem {
    ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: CollabAgentTool::SpawnAgent,
        status: ApiCollabToolCallStatus::InProgress,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids: Vec::new(),
        prompt: Some(event.prompt),
        model: Some(event.model),
        reasoning_effort: Some(event.reasoning_effort),
        agents_states: HashMap::new(),
    }
}

pub(crate) fn collab_spawn_end_item(event: CollabAgentSpawnEndEvent) -> ThreadItem {
    let has_receiver = event.new_thread_id.is_some();
    let status = if collab_agent_status_failed(&event.status) || !has_receiver {
        ApiCollabToolCallStatus::Failed
    } else {
        ApiCollabToolCallStatus::Completed
    };
    let (receiver_thread_ids, agents_states) = match event.new_thread_id {
        Some(id) => {
            let receiver_id = id.to_string();
            let received_status = ApiCollabAgentStatus::from(event.status.clone());
            (
                vec![receiver_id.clone()],
                [(receiver_id, received_status)].into_iter().collect(),
            )
        }
        None => (Vec::new(), HashMap::new()),
    };
    ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: CollabAgentTool::SpawnAgent,
        status,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids,
        prompt: Some(event.prompt),
        model: Some(event.model),
        reasoning_effort: Some(event.reasoning_effort),
        agents_states,
    }
}

pub(crate) fn collab_interaction_begin_item(event: CollabAgentInteractionBeginEvent) -> ThreadItem {
    ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: collab_interaction_tool(event.kind),
        status: ApiCollabToolCallStatus::InProgress,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids: vec![event.receiver_thread_id.to_string()],
        prompt: Some(event.prompt),
        model: None,
        reasoning_effort: None,
        agents_states: HashMap::new(),
    }
}

pub(crate) fn collab_interaction_end_item(event: CollabAgentInteractionEndEvent) -> ThreadItem {
    let status = if collab_agent_status_failed(&event.status) {
        ApiCollabToolCallStatus::Failed
    } else {
        ApiCollabToolCallStatus::Completed
    };
    let receiver_id = event.receiver_thread_id.to_string();
    let received_status = ApiCollabAgentStatus::from(event.status);
    ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: collab_interaction_tool(event.kind),
        status,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids: vec![receiver_id.clone()],
        prompt: Some(event.prompt),
        model: None,
        reasoning_effort: None,
        agents_states: [(receiver_id, received_status)].into_iter().collect(),
    }
}

pub(crate) fn collab_waiting_begin_item(event: CollabWaitingBeginEvent) -> ThreadItem {
    ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: CollabAgentTool::Wait,
        status: ApiCollabToolCallStatus::InProgress,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids: event
            .receiver_thread_ids
            .iter()
            .map(ToString::to_string)
            .collect(),
        prompt: None,
        model: None,
        reasoning_effort: None,
        agents_states: HashMap::new(),
    }
}

pub(crate) fn collab_waiting_end_item(event: CollabWaitingEndEvent) -> ThreadItem {
    let status = if event.statuses.values().any(collab_agent_status_failed) {
        ApiCollabToolCallStatus::Failed
    } else {
        ApiCollabToolCallStatus::Completed
    };
    let receiver_thread_ids = event.statuses.keys().map(ToString::to_string).collect();
    let agents_states = event
        .statuses
        .iter()
        .map(|(id, status)| (id.to_string(), ApiCollabAgentStatus::from(status.clone())))
        .collect();
    ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: CollabAgentTool::Wait,
        status,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids,
        prompt: None,
        model: None,
        reasoning_effort: None,
        agents_states,
    }
}

pub(crate) fn collab_close_begin_item(event: CollabCloseBeginEvent) -> ThreadItem {
    ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: CollabAgentTool::CloseAgent,
        status: ApiCollabToolCallStatus::InProgress,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids: vec![event.receiver_thread_id.to_string()],
        prompt: None,
        model: None,
        reasoning_effort: None,
        agents_states: HashMap::new(),
    }
}

pub(crate) fn collab_close_end_item(event: CollabCloseEndEvent) -> ThreadItem {
    let status = if collab_agent_status_failed(&event.status) {
        ApiCollabToolCallStatus::Failed
    } else {
        ApiCollabToolCallStatus::Completed
    };
    let receiver_id = event.receiver_thread_id.to_string();
    let agents_states = [(
        receiver_id.clone(),
        ApiCollabAgentStatus::from(event.status),
    )]
    .into_iter()
    .collect();
    ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: CollabAgentTool::CloseAgent,
        status,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids: vec![receiver_id],
        prompt: None,
        model: None,
        reasoning_effort: None,
        agents_states,
    }
}

pub(crate) fn collab_resume_begin_item(event: CollabResumeBeginEvent) -> ThreadItem {
    ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: CollabAgentTool::ResumeThread,
        status: ApiCollabToolCallStatus::InProgress,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids: vec![event.receiver_thread_id.to_string()],
        prompt: None,
        model: None,
        reasoning_effort: None,
        agents_states: HashMap::new(),
    }
}

pub(crate) fn collab_resume_end_item(event: CollabResumeEndEvent) -> ThreadItem {
    let status = if collab_agent_status_failed(&event.status) {
        ApiCollabToolCallStatus::Failed
    } else {
        ApiCollabToolCallStatus::Completed
    };
    let receiver_id = event.receiver_thread_id.to_string();
    let agents_states = [(
        receiver_id.clone(),
        ApiCollabAgentStatus::from(event.status),
    )]
    .into_iter()
    .collect();
    ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: CollabAgentTool::ResumeThread,
        status,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids: vec![receiver_id],
        prompt: None,
        model: None,
        reasoning_effort: None,
        agents_states,
    }
}

fn collab_interaction_tool(kind: CollabAgentInteractionKind) -> CollabAgentTool {
    match kind {
        CollabAgentInteractionKind::SendMessage => CollabAgentTool::SendMessage,
        CollabAgentInteractionKind::AssignTask => CollabAgentTool::AssignTask,
    }
}
