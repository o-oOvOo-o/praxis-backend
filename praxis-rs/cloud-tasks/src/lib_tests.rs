use super::*;
use crate::command_support::GitInfoProvider;
use crate::command_support::resolve_git_ref_with_git_info;
use crate::task_commands::AttemptDiffData;
use crate::task_commands::collect_attempt_diffs;
use crate::task_commands::format_task_list_lines;
use crate::task_commands::format_task_status_lines;
use crate::task_commands::parse_task_id;
use crate::task_commands::select_attempt;
use chrono::Utc;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use praxis_cloud_tasks_client::DiffSummary;
use praxis_cloud_tasks_client::TaskId;
use praxis_cloud_tasks_client::TaskStatus;
use praxis_cloud_tasks_client::TaskSummary;
use praxis_cloud_tasks_mock_client::MockClient;
use praxis_tui::ComposerAction;
use praxis_tui::ComposerInput;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

struct StubGitInfo {
    default_branch: Option<String>,
    current_branch: Option<String>,
}

impl StubGitInfo {
    fn new(default_branch: Option<String>, current_branch: Option<String>) -> Self {
        Self {
            default_branch,
            current_branch,
        }
    }
}

#[async_trait::async_trait]
impl GitInfoProvider for StubGitInfo {
    async fn default_branch_name(&self, _path: &std::path::Path) -> Option<String> {
        self.default_branch.clone()
    }

    async fn current_branch_name(&self, _path: &std::path::Path) -> Option<String> {
        self.current_branch.clone()
    }
}

#[tokio::test]
async fn branch_override_is_used_when_provided() {
    let git_ref = resolve_git_ref_with_git_info(
        Some(&"feature/override".to_string()),
        &StubGitInfo::new(/*default_branch*/ None, /*current_branch*/ None),
    )
    .await;

    assert_eq!(git_ref, "feature/override");
}

#[tokio::test]
async fn trims_override_whitespace() {
    let git_ref = resolve_git_ref_with_git_info(
        Some(&"  feature/spaces  ".to_string()),
        &StubGitInfo::new(/*default_branch*/ None, /*current_branch*/ None),
    )
    .await;

    assert_eq!(git_ref, "feature/spaces");
}

#[tokio::test]
async fn prefers_current_branch_when_available() {
    let git_ref = resolve_git_ref_with_git_info(
        /*branch_override*/ None,
        &StubGitInfo::new(
            Some("default-main".to_string()),
            Some("feature/current".to_string()),
        ),
    )
    .await;

    assert_eq!(git_ref, "feature/current");
}

#[tokio::test]
async fn falls_back_to_current_branch_when_default_is_missing() {
    let git_ref = resolve_git_ref_with_git_info(
        /*branch_override*/ None,
        &StubGitInfo::new(/*default_branch*/ None, Some("develop".to_string())),
    )
    .await;

    assert_eq!(git_ref, "develop");
}

#[tokio::test]
async fn falls_back_to_main_when_no_git_info_is_available() {
    let git_ref = resolve_git_ref_with_git_info(
        /*branch_override*/ None,
        &StubGitInfo::new(/*default_branch*/ None, /*current_branch*/ None),
    )
    .await;

    assert_eq!(git_ref, "main");
}

#[test]
fn format_task_status_lines_with_diff_and_label() {
    let now = Utc::now();
    let task = TaskSummary {
        id: TaskId("task_1".to_string()),
        title: "Example task".to_string(),
        status: TaskStatus::Ready,
        updated_at: now,
        environment_id: Some("env-1".to_string()),
        environment_label: Some("Env".to_string()),
        summary: DiffSummary {
            files_changed: 3,
            lines_added: 5,
            lines_removed: 2,
        },
        is_review: false,
        attempt_total: None,
    };
    let lines = format_task_status_lines(&task, now, /*colorize*/ false);
    assert_eq!(
        lines,
        vec![
            "[READY] Example task".to_string(),
            "Env  •  0s ago".to_string(),
            "+5/-2 • 3 files".to_string(),
        ]
    );
}

#[test]
fn format_task_status_lines_without_diff_falls_back() {
    let now = Utc::now();
    let task = TaskSummary {
        id: TaskId("task_2".to_string()),
        title: "No diff task".to_string(),
        status: TaskStatus::Pending,
        updated_at: now,
        environment_id: Some("env-2".to_string()),
        environment_label: None,
        summary: DiffSummary::default(),
        is_review: false,
        attempt_total: Some(1),
    };
    let lines = format_task_status_lines(&task, now, /*colorize*/ false);
    assert_eq!(
        lines,
        vec![
            "[PENDING] No diff task".to_string(),
            "env-2  •  0s ago".to_string(),
            "no diff".to_string(),
        ]
    );
}

#[test]
fn format_task_list_lines_formats_urls() {
    let now = Utc::now();
    let tasks = vec![
        TaskSummary {
            id: TaskId("task_1".to_string()),
            title: "Example task".to_string(),
            status: TaskStatus::Ready,
            updated_at: now,
            environment_id: Some("env-1".to_string()),
            environment_label: Some("Env".to_string()),
            summary: DiffSummary {
                files_changed: 3,
                lines_added: 5,
                lines_removed: 2,
            },
            is_review: false,
            attempt_total: None,
        },
        TaskSummary {
            id: TaskId("task_2".to_string()),
            title: "No diff task".to_string(),
            status: TaskStatus::Pending,
            updated_at: now,
            environment_id: Some("env-2".to_string()),
            environment_label: None,
            summary: DiffSummary::default(),
            is_review: false,
            attempt_total: Some(1),
        },
    ];
    let lines = format_task_list_lines(
        &tasks,
        "https://chatgpt.com/backend-api",
        now,
        /*colorize*/ false,
    );
    assert_eq!(
        lines,
        vec![
            "https://chatgpt.com/codex/tasks/task_1".to_string(),
            "  [READY] Example task".to_string(),
            "  Env  •  0s ago".to_string(),
            "  +5/-2 • 3 files".to_string(),
            String::new(),
            "https://chatgpt.com/codex/tasks/task_2".to_string(),
            "  [PENDING] No diff task".to_string(),
            "  env-2  •  0s ago".to_string(),
            "  no diff".to_string(),
        ]
    );
}

#[tokio::test]
async fn collect_attempt_diffs_includes_sibling_attempts() {
    let backend = MockClient;
    let task_id = parse_task_id("https://chatgpt.com/codex/tasks/T-1000").expect("id");
    let attempts = collect_attempt_diffs(&backend, &task_id)
        .await
        .expect("attempts");
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].placement, Some(0));
    assert_eq!(attempts[1].placement, Some(1));
    assert!(!attempts[0].diff.is_empty());
    assert!(!attempts[1].diff.is_empty());
}

#[test]
fn select_attempt_validates_bounds() {
    let attempts = vec![AttemptDiffData {
        placement: Some(0),
        created_at: None,
        diff: "diff --git a/file b/file\n".to_string(),
    }];
    let first = select_attempt(&attempts, Some(1)).expect("attempt 1");
    assert_eq!(first.diff, "diff --git a/file b/file\n");
    assert!(select_attempt(&attempts, Some(2)).is_err());
}

#[test]
fn parse_task_id_from_url_and_raw() {
    let raw = parse_task_id("task_i_abc123").expect("raw id");
    assert_eq!(raw.0, "task_i_abc123");
    let url =
        parse_task_id("https://chatgpt.com/codex/tasks/task_i_123456?foo=bar").expect("url id");
    assert_eq!(url.0, "task_i_123456");
    assert!(parse_task_id("   ").is_err());
}

#[test]
#[ignore = "very slow"]
fn composer_input_renders_typed_characters() {
    let mut composer = ComposerInput::new();
    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    match composer.input(key) {
        ComposerAction::Submitted(_) => panic!("unexpected submission"),
        ComposerAction::None => {}
    }

    let area = Rect::new(0, 0, 20, 5);
    let mut buf = Buffer::empty(area);
    composer.render_ref(area, &mut buf);

    let found = buf.content().iter().any(|cell| cell.symbol() == "a");
    assert!(found, "typed character was not rendered: {buf:?}");

    composer.set_hint_items(vec![("⌃O", "env"), ("⌃C", "quit")]);
    composer.render_ref(area, &mut buf);
    let footer = buf
        .content()
        .iter()
        .skip((area.width as usize) * (area.height as usize - 1))
        .map(ratatui::buffer::Cell::symbol)
        .collect::<Vec<_>>()
        .join("");
    assert!(footer.contains("⌃O env"));
}
