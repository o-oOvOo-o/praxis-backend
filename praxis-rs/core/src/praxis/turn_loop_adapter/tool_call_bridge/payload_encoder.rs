use std::collections::BTreeMap;

use serde_json::json;

use crate::tools::context::ToolPayload;

use super::super::local_shell_bridge;
use super::metadata;
use super::metadata::PayloadKind;

enum EncodedToolSearchArguments {
    Serialized(String),
    SerializationError(String),
}

impl EncodedToolSearchArguments {
    fn into_arguments(self) -> String {
        match self {
            Self::Serialized(arguments) | Self::SerializationError(arguments) => arguments,
        }
    }
}

pub(super) fn encode_payload(
    payload: ToolPayload,
    metadata: &mut BTreeMap<String, String>,
) -> String {
    let (arguments, kind) = match payload {
        ToolPayload::Function { arguments } => (arguments, PayloadKind::Function),
        ToolPayload::Mcp {
            server,
            tool,
            raw_arguments,
        } => {
            metadata::insert_mcp(metadata, server, tool);
            (raw_arguments, PayloadKind::Mcp)
        }
        ToolPayload::ToolSearch { arguments } => (
            encode_tool_search_arguments(&arguments).into_arguments(),
            PayloadKind::ToolSearch,
        ),
        ToolPayload::Custom { input } => (input, PayloadKind::Custom),
        ToolPayload::LocalShell { params } => (
            local_shell_bridge::params_to_json(&params),
            PayloadKind::LocalShell,
        ),
    };

    metadata::insert_payload_kind(metadata, kind);
    arguments
}

fn encode_tool_search_arguments<T: serde::Serialize>(arguments: &T) -> EncodedToolSearchArguments {
    serde_json::to_string(arguments).map_or_else(
        |err| {
            EncodedToolSearchArguments::SerializationError(
                json!({ "serialization_error": err.to_string() }).to_string(),
            )
        },
        EncodedToolSearchArguments::Serialized,
    )
}
