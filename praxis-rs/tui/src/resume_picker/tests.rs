use super::*;
use chrono::Duration;
use praxis_protocol::ThreadId;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use insta::assert_snapshot;
use pretty_assertions::assert_eq;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

fn line_text(line: Line<'_>) -> String {
    line.spans
        .into_iter()
        .map(|span| span.content.into_owned())
        .collect::<Vec<_>>()
        .join("")
}

fn make_row(path: &str, ts: &str, preview: &str) -> Row {
    let timestamp = parse_timestamp_str(ts);
    Row {
        path: Some(PathBuf::from(path)),
        preview: preview.to_string(),
        thread_id: None,
        thread_name: None,
        created_at: timestamp,
        updated_at: timestamp,
        cwd: None,
        git_branch: None,
    }
}

fn cursor_from_str(repr: &str) -> PageCursor {
    repr.to_string()
}

fn page(
    rows: Vec<Row>,
    next_cursor: Option<PageCursor>,
    num_scanned_files: usize,
    reached_scan_cap: bool,
) -> PickerPage {
    PickerPage {
        rows,
        next_cursor,
        num_scanned_files,
        reached_scan_cap,
    }
}

#[test]
fn row_display_preview_prefers_thread_name() {
    let row = Row {
        path: Some(PathBuf::from("/tmp/a.jsonl")),
        preview: String::from("first message"),
        thread_id: None,
        thread_name: Some(String::from("My session")),
        created_at: None,
        updated_at: None,
        cwd: None,
        git_branch: None,
    };

    assert_eq!(row.display_preview(), "My session");
}

#[test]
fn remote_thread_list_params_omit_model_providers() {
    let params = thread_list_params(
        Some(String::from("cursor-1")),
        ThreadSortKey::UpdatedAt,
        /*include_non_interactive*/ false,
        /*search_term*/ None,
        /*filter_cwd*/ None,
        ThreadArchiveFilter::Active,
    );

    assert_eq!(params.cursor, Some(String::from("cursor-1")));
    assert_eq!(params.model_providers, None);
    assert_eq!(
        params.source_kinds,
        Some(vec![ThreadSourceKind::Cli, ThreadSourceKind::VsCode])
    );
}

#[test]
fn remote_thread_list_params_can_include_non_interactive_sources() {
    let params = thread_list_params(
        Some(String::from("cursor-1")),
        ThreadSortKey::UpdatedAt,
        /*include_non_interactive*/ true,
        /*search_term*/ None,
        /*filter_cwd*/ None,
        ThreadArchiveFilter::Active,
    );

    assert_eq!(params.cursor, Some(String::from("cursor-1")));
    assert_eq!(params.model_providers, None);
    assert_eq!(params.source_kinds, None);
}

#[test]
fn remote_thread_list_params_forwards_search_term() {
    let params = thread_list_params(
        None,
        ThreadSortKey::UpdatedAt,
        /*include_non_interactive*/ false,
        Some(String::from("legacy codex")),
        /*filter_cwd*/ None,
        ThreadArchiveFilter::Active,
    );

    assert_eq!(params.search_term.as_deref(), Some("legacy codex"));
}

#[test]
fn thread_list_params_send_project_scope_without_cwd_filter() {
    let cwd = PathBuf::from("project");
    let params = thread_list_params(
        None,
        ThreadSortKey::UpdatedAt,
        /*include_non_interactive*/ false,
        /*search_term*/ None,
        Some(cwd.clone()),
        ThreadArchiveFilter::Active,
    );

    assert_eq!(params.cwd, None);
    assert_eq!(params.cwd_scope.as_deref(), Some("project"));
}

#[test]
fn picker_does_not_filter_rows_by_local_cwd() {
    let loader: PageLoader = Arc::new(|_| {});
    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ false,
        Some(PathBuf::from("/workspace/current")),
        SessionPickerAction::Resume,
    );
    state.all_rows = vec![Row {
        path: None,
        preview: String::from("remote session"),
        thread_id: Some(ThreadId::new()),
        thread_name: None,
        created_at: None,
        updated_at: None,
        cwd: Some(PathBuf::from("/srv/remote-project")),
        git_branch: None,
    }];

    state.apply_filter();

    assert_eq!(state.filtered_rows.len(), 1);
    assert_eq!(state.filtered_rows[0].preview, "remote session");
}

#[test]
fn resume_table_snapshot() {
    use crate::custom_terminal::Terminal;
    use crate::test_backend::VT100Backend;
    use ratatui::layout::Constraint;
    use ratatui::layout::Layout;

    let loader: PageLoader = Arc::new(|_| {});
    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );

    let now = Utc::now();
    let rows = vec![
        Row {
            path: Some(PathBuf::from("/tmp/a.jsonl")),
            preview: String::from("Fix resume picker timestamps"),
            thread_id: None,
            thread_name: None,
            created_at: Some(now - Duration::minutes(16)),
            updated_at: Some(now - Duration::seconds(42)),
            cwd: None,
            git_branch: None,
        },
        Row {
            path: Some(PathBuf::from("/tmp/b.jsonl")),
            preview: String::from("Investigate lazy pagination cap"),
            thread_id: None,
            thread_name: None,
            created_at: Some(now - Duration::hours(1)),
            updated_at: Some(now - Duration::minutes(35)),
            cwd: None,
            git_branch: None,
        },
        Row {
            path: Some(PathBuf::from("/tmp/c.jsonl")),
            preview: String::from("Explain the codebase"),
            thread_id: None,
            thread_name: None,
            created_at: Some(now - Duration::hours(2)),
            updated_at: Some(now - Duration::hours(2)),
            cwd: None,
            git_branch: None,
        },
    ];
    state.all_rows = rows.clone();
    state.filtered_rows = rows;
    state.view_rows = Some(3);
    state.selected = 1;
    state.scroll_top = 0;
    state.update_view_rows(/*rows*/ 3);

    let metrics = calculate_column_metrics(&state.filtered_rows, state.show_all);

    let width: u16 = 80;
    let height: u16 = 6;
    let backend = VT100Backend::new(width, height);
    let mut terminal = Terminal::with_options(backend).expect("terminal");
    terminal.set_viewport_area(Rect::new(0, 0, width, height));

    {
        let mut frame = terminal.get_frame();
        let area = frame.area();
        let segments = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
        render_column_headers(&mut frame, segments[0], &metrics, state.sort_key);
        render_list(&mut frame, segments[1], &state, &metrics);
    }
    terminal.flush().expect("flush");

    let snapshot = terminal.backend().to_string();
    assert_snapshot!("resume_picker_table", snapshot);
}

#[test]
fn resume_search_error_snapshot() {
    use crate::custom_terminal::Terminal;
    use crate::test_backend::VT100Backend;

    let loader: PageLoader = Arc::new(|_| {});
    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );
    state.inline_error = Some(String::from(
        "Failed to read session metadata from /tmp/missing.jsonl",
    ));

    let width: u16 = 80;
    let height: u16 = 1;
    let backend = VT100Backend::new(width, height);
    let mut terminal = Terminal::with_options(backend).expect("terminal");
    terminal.set_viewport_area(Rect::new(0, 0, width, height));

    {
        let mut frame = terminal.get_frame();
        let line = search_line(&state);
        frame.render_widget_ref(&line, frame.area());
    }
    terminal.flush().expect("flush");

    let snapshot = terminal.backend().to_string();
    assert_snapshot!("resume_picker_search_error", snapshot);
}

#[test]
fn resume_picker_thread_names_snapshot() {
    use crate::custom_terminal::Terminal;
    use crate::test_backend::VT100Backend;
    use ratatui::layout::Constraint;
    use ratatui::layout::Layout;

    let tempdir = tempfile::tempdir().expect("tempdir");

    let id1 = ThreadId::from_string("11111111-1111-1111-1111-111111111111").expect("thread id 1");
    let id2 = ThreadId::from_string("22222222-2222-2222-2222-222222222222").expect("thread id 2");
    let loader: PageLoader = Arc::new(|_| {});
    let mut state = PickerState::new(
        tempdir.path().to_path_buf(),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );

    let now = Utc::now();
    let rows = vec![
        Row {
            path: Some(PathBuf::from("/tmp/a.jsonl")),
            preview: String::from("First message preview"),
            thread_id: Some(id1),
            thread_name: Some(String::from("Keep this for now")),
            created_at: None,
            updated_at: Some(now - Duration::days(2)),
            cwd: None,
            git_branch: None,
        },
        Row {
            path: Some(PathBuf::from("/tmp/b.jsonl")),
            preview: String::from("Second message preview"),
            thread_id: Some(id2),
            thread_name: Some(String::from("Named thread")),
            created_at: None,
            updated_at: Some(now - Duration::days(3)),
            cwd: None,
            git_branch: None,
        },
    ];
    state.all_rows = rows.clone();
    state.filtered_rows = rows;
    state.view_rows = Some(2);
    state.selected = 0;
    state.scroll_top = 0;
    state.update_view_rows(/*rows*/ 2);

    let metrics = calculate_column_metrics(&state.filtered_rows, state.show_all);

    let width: u16 = 80;
    let height: u16 = 5;
    let backend = VT100Backend::new(width, height);
    let mut terminal = Terminal::with_options(backend).expect("terminal");
    terminal.set_viewport_area(Rect::new(0, 0, width, height));

    {
        let mut frame = terminal.get_frame();
        let area = frame.area();
        let segments = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
        render_column_headers(&mut frame, segments[0], &metrics, state.sort_key);
        render_list(&mut frame, segments[1], &state, &metrics);
    }
    terminal.flush().expect("flush");

    let snapshot = terminal.backend().to_string();
    assert_snapshot!("resume_picker_thread_names", snapshot);
}

#[test]
fn pageless_scrolling_deduplicates_and_keeps_order() {
    let loader: PageLoader = Arc::new(|_| {});
    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );

    state.reset_pagination();
    state.ingest_page(page(
        vec![
            make_row("/tmp/a.jsonl", "2025-01-03T00:00:00Z", "third"),
            make_row("/tmp/b.jsonl", "2025-01-02T00:00:00Z", "second"),
        ],
        Some(cursor_from_str(
            "2025-01-02T00-00-00|00000000-0000-0000-0000-000000000000",
        )),
        /*num_scanned_files*/ 2,
        /*reached_scan_cap*/ false,
    ));

    state.ingest_page(page(
        vec![
            make_row("/tmp/a.jsonl", "2025-01-03T00:00:00Z", "duplicate"),
            make_row("/tmp/c.jsonl", "2025-01-01T00:00:00Z", "first"),
        ],
        Some(cursor_from_str(
            "2025-01-01T00-00-00|00000000-0000-0000-0000-000000000001",
        )),
        /*num_scanned_files*/ 2,
        /*reached_scan_cap*/ false,
    ));

    state.ingest_page(page(
        vec![make_row("/tmp/d.jsonl", "2024-12-31T23:00:00Z", "very old")],
        /*next_cursor*/ None,
        /*num_scanned_files*/ 1,
        /*reached_scan_cap*/ false,
    ));

    let previews: Vec<_> = state
        .filtered_rows
        .iter()
        .map(|row| row.preview.as_str())
        .collect();
    assert_eq!(previews, vec!["third", "second", "first", "very old"]);

    let unique_paths = state
        .filtered_rows
        .iter()
        .map(|row| row.path.clone())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(unique_paths.len(), 4);
}

#[tokio::test]
async fn enter_on_load_more_requests_next_page() {
    let recorded_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let request_sink = recorded_requests.clone();
    let loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
        request_sink.lock().unwrap().push(req);
    });

    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );
    state.reset_pagination();
    state.ingest_page(page(
        vec![
            make_row("/tmp/a.jsonl", "2025-01-01T00:00:00Z", "one"),
            make_row("/tmp/b.jsonl", "2025-01-02T00:00:00Z", "two"),
        ],
        Some(cursor_from_str(
            "2025-01-03T00-00-00|00000000-0000-0000-0000-000000000000",
        )),
        /*num_scanned_files*/ 2,
        /*reached_scan_cap*/ false,
    ));

    assert!(recorded_requests.lock().unwrap().is_empty());
    state.selected = state.filtered_rows.len();
    state
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .await
        .unwrap();

    let guard = recorded_requests.lock().unwrap();
    assert_eq!(guard.len(), 1);
    assert!(guard[0].search_token.is_none());
    assert!(guard[0].cursor.is_some());
}

#[test]
fn column_visibility_hides_extra_date_column_when_narrow() {
    let metrics = ColumnMetrics {
        first_row: 0,
        max_created_width: 8,
        max_updated_width: 12,
        max_branch_width: 0,
        max_cwd_width: 0,
        labels: Vec::new(),
    };

    let created = column_visibility(/*area_width*/ 30, &metrics, ThreadSortKey::CreatedAt);
    assert_eq!(
        created,
        ColumnVisibility {
            show_created: true,
            show_updated: false,
            show_branch: false,
            show_cwd: false,
        }
    );

    let updated = column_visibility(/*area_width*/ 30, &metrics, ThreadSortKey::UpdatedAt);
    assert_eq!(
        updated,
        ColumnVisibility {
            show_created: false,
            show_updated: true,
            show_branch: false,
            show_cwd: false,
        }
    );

    let wide = column_visibility(/*area_width*/ 40, &metrics, ThreadSortKey::CreatedAt);
    assert_eq!(
        wide,
        ColumnVisibility {
            show_created: true,
            show_updated: true,
            show_branch: false,
            show_cwd: false,
        }
    );
}

#[tokio::test]
async fn toggle_sort_key_reloads_with_new_sort() {
    let recorded_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let request_sink = recorded_requests.clone();
    let loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
        request_sink.lock().unwrap().push(req);
    });

    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );

    state.start_initial_load();
    {
        let guard = recorded_requests.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert_eq!(guard[0].sort_key, ThreadSortKey::UpdatedAt);
    }

    state
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .await
        .unwrap();

    let guard = recorded_requests.lock().unwrap();
    assert_eq!(guard.len(), 2);
    assert_eq!(guard[1].sort_key, ThreadSortKey::CreatedAt);
}

#[test]
fn picker_header_and_hint_show_source_switcher() {
    let praxis_loader: PageLoader = Arc::new(|_| {});
    let codex_loader: PageLoader = Arc::new(|_| {});
    let mut state = PickerState::new(
        PathBuf::from("/tmp/praxis"),
        FrameRequester::test_dummy(),
        praxis_loader.clone(),
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );
    state.configure_source_switcher(
        SessionLookupSource::Praxis,
        SourceSwitcher::from_sources(
            SessionLookupSource::Praxis,
            PickerSourceConfig {
                praxis_home: PathBuf::from("/tmp/praxis"),
                page_loader: praxis_loader,
            },
            SessionLookupSource::Codex,
            PickerSourceConfig {
                praxis_home: PathBuf::from("/tmp/codex"),
                page_loader: codex_loader,
            },
        ),
    );

    let header = line_text(picker_header_line(&state));
    assert!(header.contains("Source:"));
    assert!(header.contains("[Praxis]"));
    assert!(header.contains("Codex"));

    let hint = line_text(picker_hint_line(&state));
    assert!(hint.contains("switch source"));
}

#[tokio::test]
async fn switching_source_reloads_other_loader_and_codex_resume_forks() {
    let praxis_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let codex_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let praxis_sink = praxis_requests.clone();
    let codex_sink = codex_requests.clone();
    let praxis_loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
        praxis_sink.lock().unwrap().push(req);
    });
    let codex_loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
        codex_sink.lock().unwrap().push(req);
    });

    let mut state = PickerState::new(
        PathBuf::from("/tmp/praxis"),
        FrameRequester::test_dummy(),
        praxis_loader.clone(),
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );
    state.configure_source_switcher(
        SessionLookupSource::Praxis,
        SourceSwitcher::from_sources(
            SessionLookupSource::Praxis,
            PickerSourceConfig {
                praxis_home: PathBuf::from("/tmp/praxis"),
                page_loader: praxis_loader,
            },
            SessionLookupSource::Codex,
            PickerSourceConfig {
                praxis_home: PathBuf::from("/tmp/codex"),
                page_loader: codex_loader,
            },
        ),
    );

    state.start_initial_load();
    assert_eq!(praxis_requests.lock().unwrap().len(), 1);
    assert!(codex_requests.lock().unwrap().is_empty());

    state
        .handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .await
        .unwrap();

    assert_eq!(state.active_source, SessionLookupSource::Codex);
    assert_eq!(codex_requests.lock().unwrap().len(), 1);

    let thread_id = ThreadId::new();
    let row = Row {
        path: Some(PathBuf::from("/tmp/codex-thread.jsonl")),
        preview: String::from("imported codex thread"),
        thread_id: Some(thread_id),
        thread_name: Some(String::from("Imported")),
        created_at: None,
        updated_at: None,
        cwd: Some(PathBuf::from("/tmp/imported-project")),
        git_branch: None,
    };
    state.all_rows = vec![row.clone()];
    state.filtered_rows = vec![row];
    state.selected = 0;

    let selection = state
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .await
        .expect("enter should not abort picker");

    match selection {
        Some(SessionSelection::Fork(SessionTarget {
            thread_id: selected_thread_id,
            thread_name: Some(thread_name),
            cwd: Some(cwd),
            ..
        })) => {
            assert_eq!(selected_thread_id, thread_id);
            assert_eq!(thread_name, "Imported");
            assert_eq!(cwd, PathBuf::from("/tmp/imported-project"));
        }
        other => panic!("unexpected selection: {other:?}"),
    }
}

#[tokio::test]
async fn page_navigation_uses_view_rows() {
    let loader: PageLoader = Arc::new(|_| {});
    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );

    let mut items = Vec::new();
    for idx in 0..20 {
        let ts = format!("2025-01-{:02}T00:00:00Z", idx + 1);
        let preview = format!("item-{idx}");
        let path = format!("/tmp/item-{idx}.jsonl");
        items.push(make_row(&path, &ts, &preview));
    }

    state.reset_pagination();
    state.ingest_page(page(
        items, /*next_cursor*/ None, /*num_scanned_files*/ 20,
        /*reached_scan_cap*/ false,
    ));
    state.update_view_rows(/*rows*/ 5);

    assert_eq!(state.selected, 0);
    state
        .handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
        .await
        .unwrap();
    assert_eq!(state.selected, 5);

    state
        .handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
        .await
        .unwrap();
    assert_eq!(state.selected, 10);

    state
        .handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
        .await
        .unwrap();
    assert_eq!(state.selected, 5);
}

#[tokio::test]
async fn enter_on_row_without_resolvable_thread_id_shows_inline_error() {
    let loader: PageLoader = Arc::new(|_| {});
    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );

    let row = Row {
        path: Some(PathBuf::from("/tmp/missing.jsonl")),
        preview: String::from("missing metadata"),
        thread_id: None,
        thread_name: None,
        created_at: None,
        updated_at: None,
        cwd: None,
        git_branch: None,
    };
    state.all_rows = vec![row.clone()];
    state.filtered_rows = vec![row];

    let selection = state
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .await
        .expect("enter should not abort the picker");

    assert!(selection.is_none());
    assert_eq!(
        state.inline_error,
        Some(String::from(
            "Failed to read session metadata from /tmp/missing.jsonl"
        ))
    );
}

#[tokio::test]
async fn enter_on_pathless_thread_uses_thread_id() {
    let loader: PageLoader = Arc::new(|_| {});
    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );
    let thread_id = ThreadId::new();
    let row = Row {
        path: None,
        preview: String::from("pathless thread"),
        thread_id: Some(thread_id),
        thread_name: None,
        created_at: None,
        updated_at: None,
        cwd: None,
        git_branch: None,
    };
    state.all_rows = vec![row.clone()];
    state.filtered_rows = vec![row];

    let selection = state
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .await
        .expect("enter should not abort the picker");

    match selection {
        Some(SessionSelection::Resume(SessionTarget {
            path: None,
            thread_id: selected_thread_id,
            thread_name: None,
            cwd: None,
        })) => assert_eq!(selected_thread_id, thread_id),
        other => panic!("unexpected selection: {other:?}"),
    }
}

#[test]
fn app_gateway_row_keeps_pathless_threads() {
    let thread_id = ThreadId::new();
    let thread = Thread {
        id: thread_id.to_string(),
        preview: String::from("remote thread"),
        summary: None,
        ephemeral: false,
        model_provider: String::from("openai"),
        model: None,
        created_at: 1,
        updated_at: 2,
        status: praxis_app_gateway_protocol::ThreadStatus::Idle,
        path: None,
        cwd: PathBuf::from("/tmp"),
        cli_version: String::from("0.0.0"),
        source: praxis_app_gateway_protocol::SessionSource::Cli,
        agent_display_name: None,
        agent_role: None,
        git_info: None,
        name: Some(String::from("Named thread")),
        total_cost_usd: None,
        last_cost_usd: None,
        token_usage: None,
        control_state: None,
        selfwork_plan_path: None,
        turns: Vec::new(),
    };

    let row = row_from_app_gateway_thread(thread).expect("row should be preserved");

    assert_eq!(row.path, None);
    assert_eq!(row.thread_id, Some(thread_id));
    assert_eq!(row.thread_name, Some(String::from("Named thread")));
}

#[tokio::test]
async fn up_at_bottom_does_not_scroll_when_visible() {
    let loader: PageLoader = Arc::new(|_| {});
    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );

    let mut items = Vec::new();
    for idx in 0..10 {
        let ts = format!("2025-02-{:02}T00:00:00Z", idx + 1);
        let preview = format!("item-{idx}");
        let path = format!("/tmp/item-{idx}.jsonl");
        items.push(make_row(&path, &ts, &preview));
    }

    state.reset_pagination();
    state.ingest_page(page(
        items, /*next_cursor*/ None, /*num_scanned_files*/ 10,
        /*reached_scan_cap*/ false,
    ));
    state.update_view_rows(/*rows*/ 5);

    state.selected = state.filtered_rows.len().saturating_sub(1);
    state.ensure_selected_visible();

    let initial_top = state.scroll_top;
    assert_eq!(initial_top, state.filtered_rows.len().saturating_sub(5));

    state
        .handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
        .await
        .unwrap();

    assert_eq!(state.scroll_top, initial_top);
    assert_eq!(state.selected, state.filtered_rows.len().saturating_sub(2));
}

#[tokio::test]
async fn set_query_restarts_backend_search_and_ignores_stale_pages() {
    let recorded_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let request_sink = recorded_requests.clone();
    let loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
        request_sink.lock().unwrap().push(req);
    });

    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );
    state.reset_pagination();
    state.ingest_page(page(
        vec![make_row(
            "/tmp/start.jsonl",
            "2025-01-01T00:00:00Z",
            "alpha",
        )],
        Some(cursor_from_str(
            "2025-01-02T00-00-00|00000000-0000-0000-0000-000000000000",
        )),
        /*num_scanned_files*/ 1,
        /*reached_scan_cap*/ false,
    ));
    recorded_requests.lock().unwrap().clear();

    state.set_query("target".to_string());
    let first_request = {
        let guard = recorded_requests.lock().unwrap();
        assert_eq!(guard.len(), 1);
        guard[0].clone()
    };
    assert!(first_request.cursor.is_none());
    assert_eq!(first_request.search_term.as_deref(), Some("target"));
    assert!(first_request.search_token.is_some());

    state.set_query("other".to_string());
    let active_request = {
        let guard = recorded_requests.lock().unwrap();
        assert_eq!(guard.len(), 2);
        guard[1].clone()
    };
    assert!(active_request.cursor.is_none());
    assert_eq!(active_request.search_term.as_deref(), Some("other"));
    assert!(active_request.search_token.is_some());

    state
        .handle_background_event(BackgroundEvent::PageLoaded {
            request_token: first_request.request_token,
            search_token: first_request.search_token,
            page: Ok(page(
                vec![make_row(
                    "/tmp/stale.jsonl",
                    "2025-01-02T00:00:00Z",
                    "target stale",
                )],
                /*next_cursor*/ None,
                /*num_scanned_files*/ 5,
                /*reached_scan_cap*/ false,
            )),
        })
        .await
        .unwrap();
    assert!(state.filtered_rows.is_empty());

    state
        .handle_background_event(BackgroundEvent::PageLoaded {
            request_token: active_request.request_token,
            search_token: active_request.search_token,
            page: Ok(page(
                vec![make_row(
                    "/tmp/backend-result.jsonl",
                    "2025-01-03T00:00:00Z",
                    "backend ranked result",
                )],
                /*next_cursor*/ None,
                /*num_scanned_files*/ 7,
                /*reached_scan_cap*/ false,
            )),
        })
        .await
        .unwrap();

    assert!(!state.filtered_rows.is_empty());
    assert!(!state.search_state.is_active());

    recorded_requests.lock().unwrap().clear();
    state.set_query(String::new());
    let clear_request = {
        let guard = recorded_requests.lock().unwrap();
        assert_eq!(guard.len(), 1);
        guard[0].clone()
    };
    assert_eq!(clear_request.search_term, None);
}

#[tokio::test]
async fn backend_search_continues_empty_pages_until_cursor_exhausted() {
    let recorded_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let request_sink = recorded_requests.clone();
    let loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
        request_sink.lock().unwrap().push(req);
    });

    let mut state = PickerState::new(
        PathBuf::from("/tmp"),
        FrameRequester::test_dummy(),
        loader,
        /*show_all*/ true,
        /*filter_cwd*/ None,
        SessionPickerAction::Resume,
    );
    state.set_query("target".to_string());
    let first_request = {
        let guard = recorded_requests.lock().unwrap();
        assert_eq!(guard.len(), 1);
        guard[0].clone()
    };

    state
        .handle_background_event(BackgroundEvent::PageLoaded {
            request_token: first_request.request_token,
            search_token: first_request.search_token,
            page: Ok(page(
                Vec::new(),
                Some(cursor_from_str(
                    "2025-01-03T00-00-00|00000000-0000-0000-0000-000000000001",
                )),
                /*num_scanned_files*/ 0,
                /*reached_scan_cap*/ false,
            )),
        })
        .await
        .unwrap();
    let second_request = {
        let guard = recorded_requests.lock().unwrap();
        assert_eq!(guard.len(), 2);
        guard[1].clone()
    };
    assert_eq!(second_request.search_term.as_deref(), Some("target"));
    assert!(second_request.cursor.is_some());

    state
        .handle_background_event(BackgroundEvent::PageLoaded {
            request_token: second_request.request_token,
            search_token: second_request.search_token,
            page: Ok(page(
                Vec::new(),
                /*next_cursor*/ None,
                /*num_scanned_files*/ 3,
                /*reached_scan_cap*/ true,
            )),
        })
        .await
        .unwrap();

    assert!(state.filtered_rows.is_empty());
    assert!(!state.search_state.is_active());
    assert!(state.pagination.reached_scan_cap);
}
