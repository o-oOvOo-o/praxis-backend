use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn background_event_updates_status_header() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_praxis_event(Event {
        id: "bg-1".into(),
        msg: EventMsg::BackgroundEvent(BackgroundEventEvent {
            message: "Waiting for `vim`".to_string(),
        }),
    });

    assert!(chat.bottom_pane.status_indicator_visible());
    assert_eq!(chat.current_status.header, "Waiting for `vim`");
    assert!(drain_insert_history(&mut rx).is_empty());
}

#[tokio::test]
async fn thread_status_snapshot_restores_distinct_active_states() {
    let cases = [
        (vec![ThreadActiveFlag::Running], GENERIC_STATUS_HEADER),
        (
            vec![ThreadActiveFlag::WaitingOnApproval],
            "Waiting for approval",
        ),
        (
            vec![ThreadActiveFlag::WaitingOnUserInput],
            "Waiting for input",
        ),
        (vec![ThreadActiveFlag::Controlled], "Controlled externally"),
    ];

    for (active_flags, expected_header) in cases {
        let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
        chat.apply_thread_status_snapshot(Some(&ThreadStatus::Active { active_flags }));

        assert!(chat.agent_turn_running);
        assert!(chat.bottom_pane.is_task_running());
        assert_eq!(chat.current_status.header, expected_header);
    }
}

#[tokio::test]
async fn idle_thread_status_snapshot_clears_restored_running_state() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.apply_thread_status_snapshot(Some(&ThreadStatus::Active {
        active_flags: vec![ThreadActiveFlag::Running],
    }));

    chat.apply_thread_status_snapshot(Some(&ThreadStatus::Idle));

    assert!(!chat.agent_turn_running);
    assert!(!chat.bottom_pane.is_task_running());
}
