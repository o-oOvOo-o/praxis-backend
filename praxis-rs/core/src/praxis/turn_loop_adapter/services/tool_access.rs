use std::sync::Arc;

use praxis_loop::services::ToolAccess;
use praxis_loop::tool::Tool;

use super::super::tool_bridge;
use super::PraxisTurnServices;

impl ToolAccess for PraxisTurnServices {
    fn resolve_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let runtime = self.tool_runtime_slot.current()?;
        tool_bridge::resolve_tool_from_runtime(&runtime, name)
    }
}
