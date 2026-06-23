use super::*;

#[tokio::test]
async fn list_threads_accepts_response_item_user_message_without_event_msg() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let ts = "2025-06-03T08-00-00";
    let uuid = Uuid::from_u128(44);
    let day_dir = home.join("sessions").join("2025").join("06").join("03");
    fs::create_dir_all(&day_dir)?;
    let file_path = day_dir.join(format!("rollout-{ts}-{uuid}.jsonl"));
    let mut file = File::create(&file_path)?;
    let thread_id = ThreadId::from_string(&uuid.to_string())?;

    let meta_line = RolloutLine {
        timestamp: ts.to_string(),
        item: RolloutItem::SessionMeta(SessionMetaLine {
            meta: SessionMeta {
                id: thread_id,
                forked_from_id: None,
                timestamp: ts.to_string(),
                cwd: ".".into(),
                originator: "test_originator".into(),
                cli_version: "test_version".into(),
                source: SessionSource::VSCode,
                agent_path: None,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                model_provider: Some(TEST_PROVIDER.into()),
                base_instructions: None,
                dynamic_tools: None,
                memory_mode: None,
            },
            git: None,
        }),
    };
    writeln!(file, "{}", serde_json::to_string(&meta_line)?)?;

    let user_response_line = RolloutLine {
        timestamp: ts.to_string(),
        item: RolloutItem::ResponseItem(ResponseItem::Message {
            id: None,
            role: "user".into(),
            content: vec![ContentItem::InputText {
                text: "Hello from response item".into(),
            }],
            end_turn: None,
            phase: None,
        }),
    };
    writeln!(file, "{}", serde_json::to_string(&user_response_line)?)?;

    let provider_filter = provider_vec(&[TEST_PROVIDER]);
    let page = get_threads(
        home,
        /*page_size*/ 1,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await?;

    let item = page.items.first().expect("conversation item");
    assert_eq!(
        item.first_user_message.as_deref(),
        Some("Hello from response item")
    );

    Ok(())
}

#[tokio::test]
async fn list_threads_skips_bootstrap_event_user_message() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let ts = "2025-06-04T08-00-00";
    let uuid = Uuid::from_u128(45);
    let day_dir = home.join("sessions").join("2025").join("06").join("04");
    fs::create_dir_all(&day_dir)?;
    let file_path = day_dir.join(format!("rollout-{ts}-{uuid}.jsonl"));
    let mut file = File::create(&file_path)?;
    let thread_id = ThreadId::from_string(&uuid.to_string())?;

    let meta_line = RolloutLine {
        timestamp: ts.to_string(),
        item: RolloutItem::SessionMeta(SessionMetaLine {
            meta: SessionMeta {
                id: thread_id,
                forked_from_id: None,
                timestamp: ts.to_string(),
                cwd: ".".into(),
                originator: "test_originator".into(),
                cli_version: "test_version".into(),
                source: SessionSource::VSCode,
                agent_path: None,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                model_provider: Some(TEST_PROVIDER.into()),
                base_instructions: None,
                dynamic_tools: None,
                memory_mode: None,
            },
            git: None,
        }),
    };
    writeln!(file, "{}", serde_json::to_string(&meta_line)?)?;

    let bootstrap_line = RolloutLine {
        timestamp: ts.to_string(),
        item: RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: format!(
                "{USER_MESSAGE_BEGIN} # AGENTS.md instructions for D:\\ghost1.0\n\n<INSTRUCTIONS>\nbody\n</INSTRUCTIONS>"
            ),
            images: Some(vec![]),
            text_elements: Vec::new(),
            local_images: Vec::new(),
        })),
    };
    writeln!(file, "{}", serde_json::to_string(&bootstrap_line)?)?;

    let real_user_line = RolloutLine {
        timestamp: ts.to_string(),
        item: RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: format!("{USER_MESSAGE_BEGIN} actual user request"),
            images: Some(vec![]),
            text_elements: Vec::new(),
            local_images: Vec::new(),
        })),
    };
    writeln!(file, "{}", serde_json::to_string(&real_user_line)?)?;

    let provider_filter = provider_vec(&[TEST_PROVIDER]);
    let page = get_threads(
        home,
        /*page_size*/ 1,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await?;

    let item = page.items.first().expect("conversation item");
    assert_eq!(
        item.first_user_message.as_deref(),
        Some("actual user request")
    );

    Ok(())
}
