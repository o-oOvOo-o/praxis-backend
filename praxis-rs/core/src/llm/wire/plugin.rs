use crate::llm::ids::WireId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WireDescriptor {
    pub(crate) id: WireId,
    pub(crate) name: &'static str,
}
