use praxis_loop::tool::ToolCall as LoopToolCall;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::praxis::turn_loop_adapter) enum PayloadKind {
    Function,
    Mcp,
    ToolSearch,
    Custom,
    LocalShell,
}

const META_PAYLOAD_KIND: &str = "praxis.payload.kind";

impl PayloadKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Mcp => "mcp",
            Self::ToolSearch => "tool_search",
            Self::Custom => "custom",
            Self::LocalShell => "local_shell",
        }
    }

    fn from_metadata(value: Option<&str>) -> Self {
        match value {
            Some("mcp") => Self::Mcp,
            Some("tool_search") => Self::ToolSearch,
            Some("custom") => Self::Custom,
            Some("local_shell") => Self::LocalShell,
            Some("function") | None | Some(_) => Self::Function,
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) fn insert_payload_kind(
    metadata: &mut std::collections::BTreeMap<String, String>,
    kind: PayloadKind,
) {
    metadata.insert(META_PAYLOAD_KIND.to_string(), kind.as_str().to_string());
}

pub(in crate::praxis::turn_loop_adapter) fn payload_kind(call: &LoopToolCall) -> PayloadKind {
    PayloadKind::from_metadata(call.metadata.get(META_PAYLOAD_KIND).map(String::as_str))
}
