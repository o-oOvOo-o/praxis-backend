//! Implements the canonical task-path based collaboration tool surface.

use crate::agent::AgentStatus;
use crate::agent::agent_resolver::resolve_agent_target;
use crate::agent::exceeds_thread_spawn_depth_limit;
use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
pub(crate) use crate::tools::handlers::multi_agents_common::build_agent_spawn_config;
use crate::tools::handlers::multi_agents_common::*;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use async_trait::async_trait;
use praxis_protocol::AgentPath;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::protocol::CollabAgentInteractionKind;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;

pub(crate) use assign_task::Handler as AssignTaskHandler;
pub(crate) use close_agent::Handler as CloseAgentHandler;
pub(crate) use list_agents::Handler as ListAgentsHandler;
pub(crate) use poll_runtime_commands::Handler as PollRuntimeCommandsHandler;
pub(crate) use read_agent_artifact::Handler as ReadAgentArtifactHandler;
pub(crate) use send_message::Handler as SendMessageHandler;
pub(crate) use spawn::Handler as SpawnAgentHandler;
pub(crate) use submit_worker_request::Handler as SubmitWorkerRequestHandler;
pub(crate) use update_runtime_command::Handler as UpdateRuntimeCommandHandler;
pub(crate) use update_worker_request::Handler as UpdateWorkerRequestHandler;
pub(crate) use wait::Handler as WaitAgentHandler;

mod assign_task;
mod close_agent;
mod events;
mod list_agents;
mod message_tool;
mod poll_runtime_commands;
mod read_agent_artifact;
mod send_message;
mod spawn;
mod submit_worker_request;
mod update_runtime_command;
mod update_worker_request;
mod wait;

use events::*;
