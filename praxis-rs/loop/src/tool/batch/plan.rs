use std::sync::Arc;

use crate::tool::PreparedToolCall;
use crate::tool::Tool;
use crate::tool::ToolCall;
use crate::tool::ToolEffects;

#[cfg(test)]
pub(crate) struct ToolExecutionPlan {
    pub(crate) nodes: Vec<PlannedTool>,
}

pub(crate) struct PlannedTool {
    pub(crate) call: ToolCall,
    pub(crate) tool: Arc<dyn Tool>,
    #[cfg(test)]
    pub(crate) dependencies: Vec<usize>,
    pub(crate) effects: ToolEffects,
    pub(crate) prepared: PreparedToolCall,
}

#[cfg(test)]
pub(crate) fn dependency_graph(effects: &[ToolEffects]) -> Vec<Vec<usize>> {
    effects
        .iter()
        .enumerate()
        .map(|(index, effect)| {
            effects[..index]
                .iter()
                .enumerate()
                .filter_map(|(prior_index, prior)| effect.conflicts(prior).then_some(prior_index))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::tool::EffectKey;
    use crate::tool::ToolEffects;

    use super::dependency_graph;

    fn file(path: &str) -> EffectKey {
        EffectKey::hierarchical("filesystem", path.split('/'))
    }

    #[test]
    fn graph_preserves_only_real_conflict_edges() {
        let graph = dependency_graph(&[
            ToolEffects::write(file("repo/a.rs")),
            ToolEffects::write(file("repo/b.rs")),
            ToolEffects::read(file("repo/a.rs")),
            ToolEffects::read(file("repo/b.rs")),
        ]);
        assert_eq!(graph, vec![vec![], vec![], vec![0], vec![1]]);
    }

    #[test]
    fn graph_keeps_provider_order_for_conflicts() {
        let graph = dependency_graph(&[
            ToolEffects::write(file("repo/a.rs")),
            ToolEffects::read(file("repo/a.rs")),
            ToolEffects::write(file("repo/a.rs")),
        ]);
        assert_eq!(graph, vec![vec![], vec![0], vec![0, 1]]);
    }
}
