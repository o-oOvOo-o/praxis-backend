use super::*;

#[test]
fn session_header_includes_reasoning_level_when_present() {
    let cell = SessionHeaderHistoryCell::new(
        "gpt-4o".to_string(),
        Some(ReasoningEffortConfig::High),
        /*show_fast_status*/ true,
        std::env::temp_dir(),
        "test",
    );

    let rendered = render_lines(&cell.display_lines(/*width*/ 80)).join("\n");
    assert!(rendered.contains("Praxis CLI"));
    assert!(rendered.contains("Tips for getting started"));
    assert!(rendered.contains("Recent activity"));
    assert!(rendered.contains("/model"));
    assert!(rendered.contains("/resume"));
    assert!(rendered.contains("No recent activity"));
    assert!(rendered.contains("gpt-4o high"));
    assert!(rendered.contains("fast"));
}

#[test]
fn session_header_hides_fast_status_when_disabled() {
    let cell = SessionHeaderHistoryCell::new(
        "gpt-4o".to_string(),
        Some(ReasoningEffortConfig::High),
        /*show_fast_status*/ false,
        std::env::temp_dir(),
        "test",
    );

    let lines = render_lines(&cell.display_lines(/*width*/ 80));
    let model_line = lines
        .iter()
        .find(|line| line.contains("gpt-4o"))
        .expect("model and billing line");

    assert!(model_line.contains("gpt-4o high"));
    assert!(!model_line.contains("fast"));
}

#[test]
fn session_header_recent_activity_uses_inline_timestamps_and_footer() {
    let mut cell = SessionHeaderHistoryCell::new(
        "gpt-4o".to_string(),
        Some(ReasoningEffortConfig::High),
        /*show_fast_status*/ false,
        std::env::temp_dir(),
        "test",
    );
    cell.recent_activity = vec![StartupRecentActivity {
        thread_id: ThreadId::new(),
        title: "Refine startup header spacing".to_string(),
        updated_at: Some(Utc::now() - chrono::Duration::minutes(3)),
    }];

    let rendered = render_lines(&cell.display_lines(/*width*/ 120)).join("\n");

    assert!(rendered.contains("3m ago  Refine startup header spacing"));
    assert!(rendered.contains("/resume for more"));
}

#[test]
fn session_header_recent_activity_aligns_titles_when_timestamp_is_missing() {
    let mut cell = SessionHeaderHistoryCell::new(
        "gpt-4o".to_string(),
        Some(ReasoningEffortConfig::High),
        /*show_fast_status*/ false,
        std::env::temp_dir(),
        "test",
    );
    cell.recent_activity = vec![
        StartupRecentActivity {
            thread_id: ThreadId::new(),
            title: "Timed thread title".to_string(),
            updated_at: Some(Utc::now() - chrono::Duration::minutes(3)),
        },
        StartupRecentActivity {
            thread_id: ThreadId::new(),
            title: "Untimed thread title".to_string(),
            updated_at: None,
        },
    ];

    let lines = render_lines(&cell.display_lines(/*width*/ 120));
    let timed_line = lines
        .iter()
        .find(|line| line.contains("Timed thread title"))
        .expect("timestamped recent activity line");
    let untimed_line = lines
        .iter()
        .find(|line| line.contains("Untimed thread title"))
        .expect("untimed recent activity line");

    let timed_title_col = timed_line
        .find("Timed thread title")
        .expect("timed title col");
    let untimed_title_col = untimed_line
        .find("Untimed thread title")
        .expect("untimed title col");

    assert_eq!(timed_title_col, untimed_title_col);
}

#[test]
fn session_header_wide_snapshot_matches_claude_style_layout() {
    let directory = if cfg!(windows) {
        PathBuf::from(r"C:\work\puppy").abs()
    } else {
        PathBuf::from("/tmp/puppy").abs()
    };
    let mut cell = SessionHeaderHistoryCell::new(
        "gpt-5.4".to_string(),
        Some(ReasoningEffortConfig::XHigh),
        /*show_fast_status*/ false,
        directory.to_path_buf(),
        "test",
    );
    cell.set_startup_notice(Some(
        "Workspace-aware resume keeps Praxis threads attached to this project.".to_string(),
    ));
    cell.recent_activity = vec![
        StartupRecentActivity {
            thread_id: ThreadId::new(),
            title: "Refine startup header spacing".to_string(),
            updated_at: None,
        },
        StartupRecentActivity {
            thread_id: ThreadId::new(),
            title: "Tune thread title generation".to_string(),
            updated_at: None,
        },
        StartupRecentActivity {
            thread_id: ThreadId::new(),
            title: "Fork keeps renamed project chat title".to_string(),
            updated_at: None,
        },
    ];

    let lines = render_lines(&cell.display_lines(/*width*/ 140));
    let rendered = lines.join("\n");
    let top_border = lines.first().expect("session header top border");
    let tips_title_idx = lines
        .iter()
        .position(|line| line.contains("Tips for getting started"))
        .expect("tips title");
    let recent_title_idx = lines
        .iter()
        .position(|line| line.contains("Recent activity"))
        .expect("recent activity title");

    assert!(rendered.contains("Praxis CLI"));
    assert!(rendered.contains("Welcome back!"));
    assert!(rendered.contains("Tips for getting started"));
    assert!(rendered.contains("Run /init to create an AGENTS.md for this repo."));
    assert!(rendered.contains("Use /resume to reopen an earlier thread."));
    assert!(rendered.contains("Use /model to switch model or reasoning effort."));
    assert!(rendered.contains("What's new"));
    assert!(
        rendered.contains("Workspace-aware resume keeps Praxis threads attached to this project.")
    );
    assert!(rendered.contains("Recent activity"));
    assert!(rendered.contains("/resume for more"));
    assert!(
        UnicodeWidthStr::width(top_border.as_str()) < 140,
        "wide welcome card should size to feed content instead of stretching edge-to-edge"
    );
    assert!(
        !lines[tips_title_idx + 1].trim().is_empty(),
        "tips feed should stay dense under its title"
    );
    assert!(
        !lines[recent_title_idx + 1].trim().is_empty(),
        "recent activity should start immediately under its title"
    );
}

#[test]
fn session_header_renders_orange_puppy_logo() {
    let cell = SessionHeaderHistoryCell::new(
        "gpt-4o".to_string(),
        Some(ReasoningEffortConfig::High),
        /*show_fast_status*/ false,
        std::env::temp_dir(),
        "test",
    );

    let lines = render_lines(&cell.display_lines(/*width*/ 80));
    assert!(
        lines.iter().any(|line| line.contains("▐██● ●██▌")),
        "expected session header to include the puppy logo"
    );
}

#[test]
fn animated_session_header_commits_resting_puppy_frame() {
    let mut cell = SessionHeaderHistoryCell::new_animated(
        "gpt-4o".to_string(),
        Some(ReasoningEffortConfig::High),
        /*show_fast_status*/ false,
        std::env::temp_dir(),
        "test",
    );
    cell.created_at =
        Instant::now() - Duration::from_millis((SESSION_HEADER_PUPPY_FRAME_MS * 39) as u64);

    let live = render_lines(&cell.display_lines(/*width*/ 80));
    let committed = render_lines(&cell.committed_display_lines(/*width*/ 80));

    assert!(live.iter().any(|line| line.contains("◡")));
    assert!(committed.iter().any(|line| line.contains("▐██● ●██▌")));
}

#[test]
fn animated_session_header_animation_loops_like_claude_code() {
    let mut cell = SessionHeaderHistoryCell::new_animated(
        "gpt-4o".to_string(),
        Some(ReasoningEffortConfig::High),
        /*show_fast_status*/ false,
        std::env::temp_dir(),
        "test",
    );
    cell.created_at = Instant::now()
        - Duration::from_millis(
            (SESSION_HEADER_PUPPY_FRAME_MS * (SESSION_HEADER_PUPPY_CYCLE_FRAMES + 26) as u128)
                as u64,
        );

    assert_eq!(
        cell.current_puppy_frame(),
        PuppyAnimationFrame {
            pose: PuppyPose::Blink,
            offset_rows: 0,
        }
    );
    assert!(cell.transcript_animation_tick().is_some());
}

#[test]
fn animated_session_header_holds_tail_wag_poses_for_two_frames() {
    assert_eq!(
        SessionHeaderHistoryCell::puppy_frame_for_cycle(6),
        PuppyAnimationFrame {
            pose: PuppyPose::WagTailRight,
            offset_rows: 0,
        }
    );
    assert_eq!(
        SessionHeaderHistoryCell::puppy_frame_for_cycle(7),
        PuppyAnimationFrame {
            pose: PuppyPose::WagTailRight,
            offset_rows: 0,
        }
    );
    assert_eq!(
        SessionHeaderHistoryCell::puppy_frame_for_cycle(8),
        PuppyAnimationFrame {
            pose: PuppyPose::WagTailLeft,
            offset_rows: 0,
        }
    );
    assert_eq!(
        SessionHeaderHistoryCell::puppy_frame_for_cycle(9),
        PuppyAnimationFrame {
            pose: PuppyPose::WagTailLeft,
            offset_rows: 0,
        }
    );
}

#[test]
fn session_header_directory_workspace_truncates() {
    let mut dir = home_dir().expect("home directory");
    for part in ["hello", "the", "fox", "is", "very", "fast"] {
        dir.push(part);
    }

    let formatted = SessionHeaderHistoryCell::format_directory_inner(&dir, Some(24));
    let sep = std::path::MAIN_SEPARATOR;
    let expected = format!("~{sep}hello{sep}the{sep}…{sep}very{sep}fast");
    assert_eq!(formatted, expected);
}

#[test]
fn session_header_directory_front_truncates_long_segment() {
    let mut dir = home_dir().expect("home directory");
    dir.push("supercalifragilisticexpialidocious");

    let formatted = SessionHeaderHistoryCell::format_directory_inner(&dir, Some(18));
    let sep = std::path::MAIN_SEPARATOR;
    let expected = format!("~{sep}…cexpialidocious");
    assert_eq!(formatted, expected);
}
