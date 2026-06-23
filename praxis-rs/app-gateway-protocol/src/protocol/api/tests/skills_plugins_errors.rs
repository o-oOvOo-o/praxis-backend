use super::*;

#[test]
fn skills_list_params_serialization_uses_force_reload() {
    assert_eq!(
        serde_json::to_value(SkillsListParams {
            cwds: Vec::new(),
            force_reload: false,
            per_cwd_extra_user_roots: None,
        })
        .unwrap(),
        json!({
            "perCwdExtraUserRoots": null,
        }),
    );

    assert_eq!(
        serde_json::to_value(SkillsListParams {
            cwds: vec![PathBuf::from("/repo")],
            force_reload: true,
            per_cwd_extra_user_roots: Some(vec![SkillsListExtraRootsForCwd {
                cwd: PathBuf::from("/repo"),
                extra_user_roots: vec![PathBuf::from("/shared/skills"), PathBuf::from("/tmp/x")],
            }]),
        })
        .unwrap(),
        json!({
            "cwds": ["/repo"],
            "forceReload": true,
            "perCwdExtraUserRoots": [
                {
                    "cwd": "/repo",
                    "extraUserRoots": ["/shared/skills", "/tmp/x"],
                }
            ],
        }),
    );
}

#[test]
fn plugin_list_params_serialization_uses_force_remote_sync() {
    assert_eq!(
        serde_json::to_value(PluginListParams {
            cwds: None,
            force_remote_sync: false,
        })
        .unwrap(),
        json!({
            "cwds": null,
        }),
    );

    assert_eq!(
        serde_json::to_value(PluginListParams {
            cwds: None,
            force_remote_sync: true,
        })
        .unwrap(),
        json!({
            "cwds": null,
            "forceRemoteSync": true,
        }),
    );
}

#[test]
fn plugin_install_params_serialization_uses_force_remote_sync() {
    let marketplace_path = if cfg!(windows) {
        r"C:\plugins\marketplace.json"
    } else {
        "/plugins/marketplace.json"
    };
    let marketplace_path = AbsolutePathBuf::try_from(PathBuf::from(marketplace_path)).unwrap();
    let marketplace_path_json = marketplace_path.as_path().display().to_string();
    assert_eq!(
        serde_json::to_value(PluginInstallParams {
            marketplace_path: marketplace_path.clone(),
            plugin_name: "gmail".to_string(),
            force_remote_sync: false,
        })
        .unwrap(),
        json!({
            "marketplacePath": marketplace_path_json,
            "pluginName": "gmail",
        }),
    );

    assert_eq!(
        serde_json::to_value(PluginInstallParams {
            marketplace_path,
            plugin_name: "gmail".to_string(),
            force_remote_sync: true,
        })
        .unwrap(),
        json!({
            "marketplacePath": marketplace_path_json,
            "pluginName": "gmail",
            "forceRemoteSync": true,
        }),
    );
}

#[test]
fn plugin_uninstall_params_serialization_uses_force_remote_sync() {
    assert_eq!(
        serde_json::to_value(PluginUninstallParams {
            plugin_id: "gmail@openai-curated".to_string(),
            force_remote_sync: false,
        })
        .unwrap(),
        json!({
            "pluginId": "gmail@openai-curated",
        }),
    );

    assert_eq!(
        serde_json::to_value(PluginUninstallParams {
            plugin_id: "gmail@openai-curated".to_string(),
            force_remote_sync: true,
        })
        .unwrap(),
        json!({
            "pluginId": "gmail@openai-curated",
            "forceRemoteSync": true,
        }),
    );
}

#[test]
fn praxis_error_info_serializes_http_status_code_in_camel_case() {
    let value = PraxisErrorInfo::ResponseTooManyFailedAttempts {
        http_status_code: Some(401),
    };

    assert_eq!(
        serde_json::to_value(value).unwrap(),
        json!({
            "responseTooManyFailedAttempts": {
                "httpStatusCode": 401
            }
        })
    );
}

#[test]
fn praxis_error_info_serializes_active_turn_not_steerable_turn_kind_in_camel_case() {
    let value = PraxisErrorInfo::ActiveTurnNotSteerable {
        turn_kind: NonSteerableTurnKind::Review,
    };

    assert_eq!(
        serde_json::to_value(value).unwrap(),
        json!({
            "activeTurnNotSteerable": {
                "turnKind": "review"
            }
        })
    );
}

#[test]
fn dynamic_tool_response_serializes_content_items() {
    let value = serde_json::to_value(DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText {
            text: "dynamic-ok".to_string(),
        }],
        success: true,
    })
    .unwrap();

    assert_eq!(
        value,
        json!({
            "contentItems": [
                {
                    "type": "inputText",
                    "text": "dynamic-ok"
                }
            ],
            "success": true,
        })
    );
}

#[test]
fn dynamic_tool_response_serializes_text_and_image_content_items() {
    let value = serde_json::to_value(DynamicToolCallResponse {
        content_items: vec![
            DynamicToolCallOutputContentItem::InputText {
                text: "dynamic-ok".to_string(),
            },
            DynamicToolCallOutputContentItem::InputImage {
                image_url: "data:image/png;base64,AAA".to_string(),
            },
        ],
        success: true,
    })
    .unwrap();

    assert_eq!(
        value,
        json!({
            "contentItems": [
                {
                    "type": "inputText",
                    "text": "dynamic-ok"
                },
                {
                    "type": "inputImage",
                    "imageUrl": "data:image/png;base64,AAA"
                }
            ],
            "success": true,
        })
    );
}
