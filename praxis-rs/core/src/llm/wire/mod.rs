pub(crate) mod claude_messages;
pub(crate) mod openai_compat;
pub(crate) mod plugin;
pub(crate) mod responses;

use plugin::WireDescriptor;

pub(crate) fn builtin_wires() -> [WireDescriptor; 3] {
    [
        responses::descriptor(),
        openai_compat::descriptor(),
        claude_messages::descriptor(),
    ]
}
