use praxis_loop::outcome::TurnError;
use praxis_loop::outcome::TurnErrorKind;
use praxis_loop::tool::ToolCall as LoopToolCall;

use super::PayloadKind;

pub(in crate::praxis::turn_loop_adapter) fn parse_arguments<T>(
    call: &LoopToolCall,
    kind: PayloadKind,
) -> Result<T, TurnError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(&call.arguments).map_err(|err| {
        TurnError::new(
            TurnErrorKind::Tool,
            format!("failed to parse {} tool arguments: {err}", kind.as_str()),
        )
    })
}
