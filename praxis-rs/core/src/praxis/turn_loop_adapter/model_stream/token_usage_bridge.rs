use praxis_loop::model::TokenUsage as LoopTokenUsage;
use praxis_protocol::protocol::TokenUsage as ProtocolTokenUsage;

pub(super) fn protocol_to_loop(token_usage: Option<&ProtocolTokenUsage>) -> LoopTokenUsage {
    let Some(token_usage) = token_usage else {
        return LoopTokenUsage::default();
    };
    LoopTokenUsage {
        input: positive_u64(token_usage.input_tokens),
        output: positive_u64(token_usage.output_tokens),
        total: positive_u64(token_usage.total_tokens),
        reasoning: positive_u64(token_usage.reasoning_output_tokens),
    }
}

fn positive_u64(value: i64) -> u64 {
    value.max(0) as u64
}
