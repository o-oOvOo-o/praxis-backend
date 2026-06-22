use std::sync::Arc;

use crate::tool::Tool;
use crate::tool::ToolRegistry;

pub trait ToolAccess: Send + Sync {
    fn resolve_tool(&self, name: &str) -> Option<Arc<dyn Tool>>;
}

impl<T> ToolAccess for T
where
    T: ToolRegistry + Send + Sync,
{
    fn resolve_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.get(name)
    }
}
