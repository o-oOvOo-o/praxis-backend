use super::*;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn create_apply_patch_freeform_tool_matches_expected_spec() {
    assert_eq!(
        create_apply_patch_freeform_tool(),
        ToolSpec::Freeform(FreeformTool {
            name: "apply_patch".to_string(),
            description: APPLY_PATCH_FREEFORM_TOOL_DESCRIPTION.to_string(),
            format: FreeformToolFormat {
                r#type: "grammar".to_string(),
                syntax: "lark".to_string(),
                definition: APPLY_PATCH_LARK_GRAMMAR.to_string(),
            },
        })
    );
}

#[test]
fn create_apply_patch_json_tool_matches_expected_spec() {
    assert_eq!(
        create_apply_patch_json_tool(),
        ToolSpec::Function(ResponsesApiTool {
            name: "apply_patch".to_string(),
            description: APPLY_PATCH_JSON_TOOL_DESCRIPTION.to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([(
                    "input".to_string(),
                    JsonSchema::String {
                        description: Some(
                            "The entire contents of the apply_patch command".to_string(),
                        ),
                    },
                )]),
                required: Some(vec!["input".to_string()]),
                additional_properties: Some(false.into()),
            },
            output_schema: None,
        })
    );
}

#[test]
fn apply_patch_args_accept_patch_alias_from_common_providers() {
    let args: ApplyPatchToolArgs = serde_json::from_value(serde_json::json!({
        "paths": ["D:\\ghost1.0\\Cunning3D_1.0\\example.rs"],
        "patch": "*** Begin Patch\n*** End Patch\n"
    }))
    .expect("patch alias should deserialize");

    assert_eq!(args.input, "*** Begin Patch\n*** End Patch\n");
}
