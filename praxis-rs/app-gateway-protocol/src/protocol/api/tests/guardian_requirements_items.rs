use super::*;

#[test]
fn sandbox_policy_round_trips_workspace_write_read_only_access() {
    let readable_root = test_absolute_path();
    let api_policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: ReadOnlyAccess::Restricted {
            include_platform_defaults: false,
            readable_roots: vec![readable_root.clone()],
        },
        network_access: true,
        exclude_tmpdir_env_var: false,
        exclude_slash_tmp: false,
    };

    let core_policy = api_policy.to_core();
    assert_eq!(
        core_policy,
        praxis_protocol::protocol::SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![],
            read_only_access: CoreReadOnlyAccess::Restricted {
                include_platform_defaults: false,
                readable_roots: vec![readable_root],
            },
            network_access: true,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
        }
    );

    let back_to_api = SandboxPolicy::from(core_policy);
    assert_eq!(back_to_api, api_policy);
}

#[test]
fn automatic_approval_review_deserializes_aborted_status() {
    let review: GuardianApprovalReview = serde_json::from_value(json!({
        "status": "aborted",
        "riskScore": null,
        "riskLevel": null,
        "rationale": null
    }))
    .expect("aborted automatic review should deserialize");
    assert_eq!(
        review,
        GuardianApprovalReview {
            status: GuardianApprovalReviewStatus::Aborted,
            risk_score: None,
            risk_level: None,
            rationale: None,
        }
    );
}

#[test]
fn guardian_approval_review_action_round_trips_command_shape() {
    let value = json!({
        "type": "command",
        "source": "shell",
        "command": "rm -rf /tmp/example.sqlite",
        "cwd": "/tmp",
    });
    let action: GuardianApprovalReviewAction =
        serde_json::from_value(value.clone()).expect("guardian review action");

    assert_eq!(
        action,
        GuardianApprovalReviewAction::Command {
            source: GuardianCommandSource::Shell,
            command: "rm -rf /tmp/example.sqlite".to_string(),
            cwd: "/tmp".into(),
        }
    );
    assert_eq!(
        serde_json::to_value(&action).expect("serialize guardian review action"),
        value
    );
}

#[test]
fn network_requirements_serializes_canonical_fields() {
    let requirements = NetworkRequirements {
        enabled: Some(true),
        http_port: Some(8080),
        socks_port: Some(1080),
        allow_upstream_proxy: Some(false),
        dangerously_allow_non_loopback_proxy: Some(false),
        dangerously_allow_all_unix_sockets: Some(true),
        domains: Some(BTreeMap::from([
            ("api.openai.com".to_string(), NetworkDomainPermission::Allow),
            (
                "blocked.example.com".to_string(),
                NetworkDomainPermission::Deny,
            ),
        ])),
        managed_allowed_domains_only: Some(true),
        unix_sockets: Some(BTreeMap::from([
            (
                "/tmp/proxy.sock".to_string(),
                NetworkUnixSocketPermission::Allow,
            ),
            (
                "/tmp/ignored.sock".to_string(),
                NetworkUnixSocketPermission::None,
            ),
        ])),
        allow_local_binding: Some(true),
    };

    assert_eq!(
        serde_json::to_value(requirements).expect("network requirements should serialize"),
        json!({
            "enabled": true,
            "httpPort": 8080,
            "socksPort": 1080,
            "allowUpstreamProxy": false,
            "dangerouslyAllowNonLoopbackProxy": false,
            "dangerouslyAllowAllUnixSockets": true,
            "domains": {
                "api.openai.com": "allow",
                "blocked.example.com": "deny"
            },
            "managedAllowedDomainsOnly": true,
            "unixSockets": {
                "/tmp/ignored.sock": "none",
                "/tmp/proxy.sock": "allow"
            },
            "allowLocalBinding": true
        })
    );
}

#[test]
fn core_turn_item_into_thread_item_converts_supported_variants() {
    let user_item = TurnItem::UserMessage(UserMessageItem {
        id: "user-1".to_string(),
        content: vec![
            CoreUserInput::Text {
                text: "hello".to_string(),
                text_elements: Vec::new(),
            },
            CoreUserInput::Image {
                image_url: "https://example.com/image.png".to_string(),
            },
            CoreUserInput::LocalImage {
                path: PathBuf::from("local/image.png"),
            },
            CoreUserInput::Skill {
                name: "skill-creator".to_string(),
                path: PathBuf::from("/repo/.praxis/skills/skill-creator/SKILL.md"),
            },
            CoreUserInput::Mention {
                name: "Demo App".to_string(),
                path: "app://demo-app".to_string(),
            },
        ],
    });

    assert_eq!(
        ThreadItem::from(user_item),
        ThreadItem::UserMessage {
            id: "user-1".to_string(),
            content: vec![
                UserInput::Text {
                    text: "hello".to_string(),
                    text_elements: Vec::new(),
                },
                UserInput::Image {
                    url: "https://example.com/image.png".to_string(),
                },
                UserInput::LocalImage {
                    path: PathBuf::from("local/image.png"),
                },
                UserInput::Skill {
                    name: "skill-creator".to_string(),
                    path: PathBuf::from("/repo/.praxis/skills/skill-creator/SKILL.md"),
                },
                UserInput::Mention {
                    name: "Demo App".to_string(),
                    path: "app://demo-app".to_string(),
                },
            ],
        }
    );

    let agent_item = TurnItem::AgentMessage(AgentMessageItem {
        id: "agent-1".to_string(),
        content: vec![
            AgentMessageContent::Text {
                text: "Hello ".to_string(),
            },
            AgentMessageContent::Text {
                text: "world".to_string(),
            },
        ],
        phase: None,
        memory_citation: None,
    });

    assert_eq!(
        ThreadItem::from(agent_item),
        ThreadItem::AgentMessage {
            id: "agent-1".to_string(),
            text: "Hello world".to_string(),
            phase: None,
            memory_citation: None,
        }
    );

    let agent_item_with_phase = TurnItem::AgentMessage(AgentMessageItem {
        id: "agent-2".to_string(),
        content: vec![AgentMessageContent::Text {
            text: "final".to_string(),
        }],
        phase: Some(MessagePhase::FinalAnswer),
        memory_citation: Some(CoreMemoryCitation {
            entries: vec![CoreMemoryCitationEntry {
                path: "MEMORY.md".to_string(),
                line_start: 1,
                line_end: 2,
                note: "summary".to_string(),
            }],
            rollout_ids: vec!["rollout-1".to_string()],
        }),
    });

    assert_eq!(
        ThreadItem::from(agent_item_with_phase),
        ThreadItem::AgentMessage {
            id: "agent-2".to_string(),
            text: "final".to_string(),
            phase: Some(MessagePhase::FinalAnswer),
            memory_citation: Some(MemoryCitation {
                entries: vec![MemoryCitationEntry {
                    path: "MEMORY.md".to_string(),
                    line_start: 1,
                    line_end: 2,
                    note: "summary".to_string(),
                }],
                thread_ids: vec!["rollout-1".to_string()],
            }),
        }
    );

    let reasoning_item = TurnItem::Reasoning(ReasoningItem {
        id: "reasoning-1".to_string(),
        summary_text: vec!["line one".to_string(), "line two".to_string()],
        raw_content: vec![],
    });

    assert_eq!(
        ThreadItem::from(reasoning_item),
        ThreadItem::Reasoning {
            id: "reasoning-1".to_string(),
            summary: vec!["line one".to_string(), "line two".to_string()],
            content: vec![],
        }
    );

    let search_item = TurnItem::WebSearch(WebSearchItem {
        id: "search-1".to_string(),
        query: "docs".to_string(),
        action: CoreWebSearchAction::Search {
            query: Some("docs".to_string()),
            queries: None,
        },
    });

    assert_eq!(
        ThreadItem::from(search_item),
        ThreadItem::WebSearch {
            id: "search-1".to_string(),
            query: "docs".to_string(),
            action: Some(WebSearchAction::Search {
                query: Some("docs".to_string()),
                queries: None,
            }),
        }
    );
}
