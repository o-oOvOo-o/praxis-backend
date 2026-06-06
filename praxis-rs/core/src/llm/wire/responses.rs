use super::plugin::WireDescriptor;
use crate::llm::ids::WireId;

pub(crate) fn descriptor() -> WireDescriptor {
    WireDescriptor {
        id: WireId::Responses,
        name: "OpenAI Responses",
    }
}
