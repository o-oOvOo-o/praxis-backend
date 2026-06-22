use crate::agent_os::model::ActionIntent;
use crate::agent_os::model::ActionIntentKind;
use crate::agent_os::model::ResourceRequirement;

pub(in crate::agent_os) fn classify_mutating_tool(tool_name: &str) -> ActionIntent {
    ActionIntent {
        kind: ActionIntentKind::UnknownRisky,
        confidence: 0.50,
        required_resources: vec![ResourceRequirement::Network {
            scope: "external_tool".to_string(),
        }],
        side_effects: vec![format!("mutating external tool `{tool_name}`")],
        risk_level: "high".to_string(),
    }
}
