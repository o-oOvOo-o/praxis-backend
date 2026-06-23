use super::*;
use crate::app_event::AppEvent;
use insta::assert_snapshot;
use praxis_protocol::models::FileSystemPermissions;
use praxis_protocol::models::NetworkPermissions;
use praxis_protocol::protocol::ExecPolicyAmendment;
use praxis_protocol::protocol::NetworkApprovalProtocol;
use praxis_protocol::protocol::NetworkPolicyAmendment;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use tokio::sync::mpsc::unbounded_channel;

fn absolute_path(path: &str) -> AbsolutePathBuf {
    AbsolutePathBuf::from_absolute_path(path).expect("absolute path")
}

fn render_overlay_lines(view: &ApprovalOverlay, width: u16) -> String {
    let height = view.desired_height(width);
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
    view.render(Rect::new(0, 0, width, height), &mut buf);
    (0..buf.area.height)
        .map(|row| {
            (0..buf.area.width)
                .map(|col| buf[(col, row)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_snapshot_paths(rendered: String) -> String {
    [
        (absolute_path("/tmp/readme.txt"), "/tmp/readme.txt"),
        (absolute_path("/tmp/out.txt"), "/tmp/out.txt"),
    ]
    .into_iter()
    .fold(rendered, |rendered, (path, normalized)| {
        rendered.replace(&path.display().to_string(), normalized)
    })
}

fn make_exec_request() -> ApprovalRequest {
    ApprovalRequest::Exec {
        thread_id: ThreadId::new(),
        thread_label: None,
        id: "test".to_string(),
        command: vec!["echo".to_string(), "hi".to_string()],
        reason: Some("reason".to_string()),
        available_decisions: vec![ReviewDecision::Approved, ReviewDecision::Abort],
        network_approval_context: None,
        additional_permissions: None,
    }
}

fn make_permissions_request() -> ApprovalRequest {
    ApprovalRequest::Permissions {
        thread_id: ThreadId::new(),
        thread_label: None,
        call_id: "test".to_string(),
        reason: Some("need workspace access".to_string()),
        permissions: RequestPermissionProfile {
            network: Some(NetworkPermissions {
                enabled: Some(true),
            }),
            file_system: Some(FileSystemPermissions {
                read: Some(vec![absolute_path("/tmp/readme.txt")]),
                write: Some(vec![absolute_path("/tmp/out.txt")]),
            }),
        },
    }
}

#[test]
fn ctrl_c_aborts_and_clears_queue() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let mut view = ApprovalOverlay::new(make_exec_request(), tx, Features::with_defaults());
    view.enqueue_request(make_exec_request());
    assert_eq!(CancellationEvent::Handled, view.on_ctrl_c());
    assert!(view.queue.is_empty());
    assert!(view.is_complete());
}

#[test]
fn shortcut_triggers_selection() {
    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let mut view = ApprovalOverlay::new(make_exec_request(), tx, Features::with_defaults());
    assert!(!view.is_complete());
    view.handle_key_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
    // We expect at least one thread-scoped approval op message in the queue.
    let mut saw_op = false;
    while let Ok(ev) = rx.try_recv() {
        if matches!(ev, AppEvent::SubmitThreadOp { .. }) {
            saw_op = true;
            break;
        }
    }
    assert!(saw_op, "expected approval decision to emit an op");
}

#[test]
fn o_opens_source_thread_for_cross_thread_approval() {
    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let thread_id = ThreadId::new();
    let mut view = ApprovalOverlay::new(
        ApprovalRequest::Exec {
            thread_id,
            thread_label: Some("Robie [explorer]".to_string()),
            id: "test".to_string(),
            command: vec!["echo".to_string(), "hi".to_string()],
            reason: None,
            available_decisions: vec![ReviewDecision::Approved, ReviewDecision::Abort],
            network_approval_context: None,
            additional_permissions: None,
        },
        tx,
        Features::with_defaults(),
    );

    view.handle_key_event(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

    let event = rx.try_recv().expect("expected select-agent-thread event");
    assert_eq!(
        matches!(event, AppEvent::SelectAgentThread(id) if id == thread_id),
        true
    );
}

#[test]
fn cross_thread_footer_hint_mentions_o_shortcut() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let view = ApprovalOverlay::new(
        ApprovalRequest::Exec {
            thread_id: ThreadId::new(),
            thread_label: Some("Robie [explorer]".to_string()),
            id: "test".to_string(),
            command: vec!["echo".to_string(), "hi".to_string()],
            reason: None,
            available_decisions: vec![ReviewDecision::Approved, ReviewDecision::Abort],
            network_approval_context: None,
            additional_permissions: None,
        },
        tx,
        Features::with_defaults(),
    );

    assert_snapshot!(
        "approval_overlay_cross_thread_prompt",
        render_overlay_lines(&view, /*width*/ 80)
    );
}

#[test]
fn exec_prefix_option_emits_execpolicy_amendment() {
    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let mut view = ApprovalOverlay::new(
        ApprovalRequest::Exec {
            thread_id: ThreadId::new(),
            thread_label: None,
            id: "test".to_string(),
            command: vec!["echo".to_string()],
            reason: None,
            available_decisions: vec![
                ReviewDecision::Approved,
                ReviewDecision::ApprovedExecpolicyAmendment {
                    proposed_execpolicy_amendment: ExecPolicyAmendment::new(vec![
                        "echo".to_string(),
                    ]),
                },
                ReviewDecision::Abort,
            ],
            network_approval_context: None,
            additional_permissions: None,
        },
        tx,
        Features::with_defaults(),
    );
    view.handle_key_event(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let mut saw_op = false;
    while let Ok(ev) = rx.try_recv() {
        if let AppEvent::SubmitThreadOp {
            op: Op::ExecApproval { decision, .. },
            ..
        } = ev
        {
            assert_eq!(
                decision,
                ReviewDecision::ApprovedExecpolicyAmendment {
                    proposed_execpolicy_amendment: ExecPolicyAmendment::new(vec![
                        "echo".to_string()
                    ])
                }
            );
            saw_op = true;
            break;
        }
    }
    assert!(
        saw_op,
        "expected approval decision to emit an op with command prefix"
    );
}

#[test]
fn network_deny_forever_shortcut_is_not_bound() {
    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let mut view = ApprovalOverlay::new(
        ApprovalRequest::Exec {
            thread_id: ThreadId::new(),
            thread_label: None,
            id: "test".to_string(),
            command: vec!["curl".to_string(), "https://example.com".to_string()],
            reason: None,
            available_decisions: vec![
                ReviewDecision::Approved,
                ReviewDecision::ApprovedForSession,
                ReviewDecision::NetworkPolicyAmendment {
                    network_policy_amendment: NetworkPolicyAmendment {
                        host: "example.com".to_string(),
                        action: NetworkPolicyRuleAction::Allow,
                    },
                },
                ReviewDecision::Abort,
            ],
            network_approval_context: Some(NetworkApprovalContext {
                host: "example.com".to_string(),
                protocol: NetworkApprovalProtocol::Https,
            }),
            additional_permissions: None,
        },
        tx,
        Features::with_defaults(),
    );
    view.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

    assert!(
        rx.try_recv().is_err(),
        "unexpected approval event emitted for hidden network deny shortcut"
    );
}

#[test]
fn header_includes_command_snippet() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let command = vec!["echo".into(), "hello".into(), "world".into()];
    let exec_request = ApprovalRequest::Exec {
        thread_id: ThreadId::new(),
        thread_label: None,
        id: "test".into(),
        command,
        reason: None,
        available_decisions: vec![ReviewDecision::Approved, ReviewDecision::Abort],
        network_approval_context: None,
        additional_permissions: None,
    };

    let view = ApprovalOverlay::new(exec_request, tx, Features::with_defaults());
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, view.desired_height(/*width*/ 80)));
    view.render(
        Rect::new(0, 0, 80, view.desired_height(/*width*/ 80)),
        &mut buf,
    );

    let rendered: Vec<String> = (0..buf.area.height)
        .map(|row| {
            (0..buf.area.width)
                .map(|col| buf[(col, row)].symbol().to_string())
                .collect()
        })
        .collect();
    assert!(
        rendered
            .iter()
            .any(|line| line.contains("echo hello world")),
        "expected header to include command snippet, got {rendered:?}"
    );
}

#[test]
fn network_exec_options_use_expected_labels_and_hide_execpolicy_amendment() {
    let network_context = NetworkApprovalContext {
        host: "example.com".to_string(),
        protocol: NetworkApprovalProtocol::Https,
    };
    let options = exec_options(
        &[
            ReviewDecision::Approved,
            ReviewDecision::ApprovedForSession,
            ReviewDecision::NetworkPolicyAmendment {
                network_policy_amendment: NetworkPolicyAmendment {
                    host: "example.com".to_string(),
                    action: NetworkPolicyRuleAction::Allow,
                },
            },
            ReviewDecision::Abort,
        ],
        Some(&network_context),
        /*additional_permissions*/ None,
    );

    let labels: Vec<String> = options.into_iter().map(|option| option.label).collect();
    assert_eq!(
        labels,
        vec![
            "Yes, just this once".to_string(),
            "Yes, and allow this host for this conversation".to_string(),
            "Yes, and allow this host in the future".to_string(),
            "No, and tell Praxis what to do differently".to_string(),
        ]
    );
}

#[test]
fn generic_exec_options_can_offer_allow_for_session() {
    let options = exec_options(
        &[
            ReviewDecision::Approved,
            ReviewDecision::ApprovedForSession,
            ReviewDecision::Abort,
        ],
        /*network_approval_context*/ None,
        /*additional_permissions*/ None,
    );

    let labels: Vec<String> = options.into_iter().map(|option| option.label).collect();
    assert_eq!(
        labels,
        vec![
            "Yes, proceed".to_string(),
            "Yes, and don't ask again for this command in this session".to_string(),
            "No, and tell Praxis what to do differently".to_string(),
        ]
    );
}

#[test]
fn additional_permissions_exec_options_hide_execpolicy_amendment() {
    let additional_permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![absolute_path("/tmp/readme.txt")]),
            write: Some(vec![absolute_path("/tmp/out.txt")]),
        }),
        ..Default::default()
    };
    let options = exec_options(
        &[ReviewDecision::Approved, ReviewDecision::Abort],
        /*network_approval_context*/ None,
        Some(&additional_permissions),
    );

    let labels: Vec<String> = options.into_iter().map(|option| option.label).collect();
    assert_eq!(
        labels,
        vec![
            "Yes, proceed".to_string(),
            "No, and tell Praxis what to do differently".to_string(),
        ]
    );
}

#[test]
fn permissions_options_use_expected_labels() {
    let labels: Vec<String> = permissions_options()
        .into_iter()
        .map(|option| option.label)
        .collect();
    assert_eq!(
        labels,
        vec![
            "Yes, grant these permissions".to_string(),
            "Yes, grant these permissions for this session".to_string(),
            "No, continue without permissions".to_string(),
        ]
    );
}

#[test]
fn permissions_session_shortcut_submits_session_scope() {
    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let mut view = ApprovalOverlay::new(make_permissions_request(), tx, Features::with_defaults());

    view.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));

    let mut saw_op = false;
    while let Ok(ev) = rx.try_recv() {
        if let AppEvent::SubmitThreadOp {
            op: Op::RequestPermissionsResponse { response, .. },
            ..
        } = ev
        {
            assert_eq!(response.scope, PermissionGrantScope::Session);
            saw_op = true;
            break;
        }
    }
    assert!(
        saw_op,
        "expected permission approval decision to emit a session-scoped response"
    );
}

#[test]
fn additional_permissions_prompt_shows_permission_rule_line() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let exec_request = ApprovalRequest::Exec {
        thread_id: ThreadId::new(),
        thread_label: None,
        id: "test".into(),
        command: vec!["cat".into(), "/tmp/readme.txt".into()],
        reason: None,
        available_decisions: vec![ReviewDecision::Approved, ReviewDecision::Abort],
        network_approval_context: None,
        additional_permissions: Some(PermissionProfile {
            network: Some(NetworkPermissions {
                enabled: Some(true),
            }),
            file_system: Some(FileSystemPermissions {
                read: Some(vec![absolute_path("/tmp/readme.txt")]),
                write: Some(vec![absolute_path("/tmp/out.txt")]),
            }),
        }),
    };

    let view = ApprovalOverlay::new(exec_request, tx, Features::with_defaults());
    let mut buf = Buffer::empty(Rect::new(0, 0, 120, view.desired_height(/*width*/ 120)));
    view.render(
        Rect::new(0, 0, 120, view.desired_height(/*width*/ 120)),
        &mut buf,
    );

    let rendered: Vec<String> = (0..buf.area.height)
        .map(|row| {
            (0..buf.area.width)
                .map(|col| buf[(col, row)].symbol().to_string())
                .collect()
        })
        .collect();

    assert!(
        rendered
            .iter()
            .any(|line| line.contains("Permission rule:")),
        "expected permission-rule line, got {rendered:?}"
    );
    assert!(
        rendered.iter().any(|line| line.contains("network;")),
        "expected network permission text, got {rendered:?}"
    );
}

#[test]
fn additional_permissions_prompt_snapshot() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let exec_request = ApprovalRequest::Exec {
        thread_id: ThreadId::new(),
        thread_label: None,
        id: "test".into(),
        command: vec!["cat".into(), "/tmp/readme.txt".into()],
        reason: Some("need filesystem access".into()),
        available_decisions: vec![ReviewDecision::Approved, ReviewDecision::Abort],
        network_approval_context: None,
        additional_permissions: Some(PermissionProfile {
            network: Some(NetworkPermissions {
                enabled: Some(true),
            }),
            file_system: Some(FileSystemPermissions {
                read: Some(vec![absolute_path("/tmp/readme.txt")]),
                write: Some(vec![absolute_path("/tmp/out.txt")]),
            }),
        }),
    };

    let view = ApprovalOverlay::new(exec_request, tx, Features::with_defaults());
    assert_snapshot!(
        "approval_overlay_additional_permissions_prompt",
        normalize_snapshot_paths(render_overlay_lines(&view, /*width*/ 120))
    );
}

#[test]
fn permissions_prompt_snapshot() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let view = ApprovalOverlay::new(make_permissions_request(), tx, Features::with_defaults());
    assert_snapshot!(
        "approval_overlay_permissions_prompt",
        normalize_snapshot_paths(render_overlay_lines(&view, /*width*/ 120))
    );
}

#[test]
fn network_exec_prompt_title_includes_host() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx);
    let exec_request = ApprovalRequest::Exec {
        thread_id: ThreadId::new(),
        thread_label: None,
        id: "test".into(),
        command: vec!["curl".into(), "https://example.com".into()],
        reason: Some("network request blocked".into()),
        available_decisions: vec![
            ReviewDecision::Approved,
            ReviewDecision::ApprovedForSession,
            ReviewDecision::NetworkPolicyAmendment {
                network_policy_amendment: NetworkPolicyAmendment {
                    host: "example.com".to_string(),
                    action: NetworkPolicyRuleAction::Allow,
                },
            },
            ReviewDecision::Abort,
        ],
        network_approval_context: Some(NetworkApprovalContext {
            host: "example.com".to_string(),
            protocol: NetworkApprovalProtocol::Https,
        }),
        additional_permissions: None,
    };

    let view = ApprovalOverlay::new(exec_request, tx, Features::with_defaults());
    let mut buf = Buffer::empty(Rect::new(0, 0, 100, view.desired_height(/*width*/ 100)));
    view.render(
        Rect::new(0, 0, 100, view.desired_height(/*width*/ 100)),
        &mut buf,
    );
    assert_snapshot!("network_exec_prompt", format!("{buf:?}"));

    let rendered: Vec<String> = (0..buf.area.height)
        .map(|row| {
            (0..buf.area.width)
                .map(|col| buf[(col, row)].symbol().to_string())
                .collect()
        })
        .collect();

    assert!(
        rendered.iter().any(|line| {
            line.contains("Do you want to approve network access to \"example.com\"?")
        }),
        "expected network title to include host, got {rendered:?}"
    );
    assert!(
        !rendered.iter().any(|line| line.contains("$ curl")),
        "network prompt should not show command line, got {rendered:?}"
    );
    assert!(
        !rendered.iter().any(|line| line.contains("don't ask again")),
        "network prompt should not show execpolicy option, got {rendered:?}"
    );
}

#[test]
fn exec_history_cell_wraps_with_two_space_indent() {
    let command = vec![
        "/bin/zsh".into(),
        "-lc".into(),
        "git add tui/src/render/mod.rs tui/src/render/renderable.rs".into(),
    ];
    let cell = history_cell::new_approval_decision_cell(
        command,
        ReviewDecision::Approved,
        history_cell::ApprovalDecisionActor::User,
    );
    let lines = cell.display_lines(/*width*/ 28);
    let rendered: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect();
    let expected = vec![
        "✔ You approved Praxis to run".to_string(),
        "  git add tui/src/render/".to_string(),
        "  mod.rs tui/src/render/".to_string(),
        "  renderable.rs this time".to_string(),
    ];
    assert_eq!(rendered, expected);
}

#[test]
fn enter_sets_last_selected_index_without_dismissing() {
    let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut view = ApprovalOverlay::new(make_exec_request(), tx, Features::with_defaults());
    view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        view.is_complete(),
        "exec approval should complete without queued requests"
    );

    let mut decision = None;
    while let Ok(ev) = rx.try_recv() {
        if let AppEvent::SubmitThreadOp {
            op: Op::ExecApproval { decision: d, .. },
            ..
        } = ev
        {
            decision = Some(d);
            break;
        }
    }
    assert_eq!(decision, Some(ReviewDecision::Approved));
}
