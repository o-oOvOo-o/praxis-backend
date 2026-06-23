use super::*;
use crate::exit_handling::format_exit_messages;
use assert_matches::assert_matches;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::TokenUsage;
use praxis_tui::AppExitInfo;
use praxis_tui::Cli as TuiCli;
use praxis_tui::ExitReason;
use pretty_assertions::assert_eq;

fn finalize_resume_from_args(args: &[&str]) -> TuiCli {
    let cli = MultitoolCli::try_parse_from(args).expect("parse");
    let MultitoolCli {
        interactive,
        config_overrides: root_overrides,
        subcommand,
        feature_toggles: _,
        remote: _,
    } = cli;

    let Subcommand::Resume(ResumeCommand {
        target,
        target_extra,
        last,
        all,
        include_non_interactive,
        remote: _,
        config_overrides: resume_cli,
    }) = subcommand.expect("resume present")
    else {
        unreachable!()
    };

    let targets = collect_session_target_args(target, target_extra);
    let parsed_target = parse_session_target_args(targets, "resume").expect("parse target");

    finalize_resume_interactive(
        interactive,
        root_overrides,
        parsed_target.session_id,
        parsed_target.source.lookup_source(),
        last,
        all,
        include_non_interactive,
        resume_cli,
    )
}

fn finalize_fork_from_args(args: &[&str]) -> TuiCli {
    let cli = MultitoolCli::try_parse_from(args).expect("parse");
    let MultitoolCli {
        interactive,
        config_overrides: root_overrides,
        subcommand,
        feature_toggles: _,
        remote: _,
    } = cli;

    let Subcommand::Fork(ForkCommand {
        target,
        target_extra,
        last,
        all,
        remote: _,
        config_overrides: fork_cli,
    }) = subcommand.expect("fork present")
    else {
        unreachable!()
    };

    let targets = collect_session_target_args(target, target_extra);
    let parsed_target = parse_session_target_args(targets, "fork").expect("parse target");

    finalize_fork_interactive(
        interactive,
        root_overrides,
        parsed_target.session_id,
        parsed_target.source.lookup_source(),
        last,
        all,
        fork_cli,
    )
}

fn finalize_dev_from_args(args: &[&str]) -> TuiCli {
    let cli = MultitoolCli::try_parse_from(args).expect("parse");
    let MultitoolCli {
        interactive,
        config_overrides: root_overrides,
        subcommand,
        feature_toggles: _,
        remote: _,
    } = cli;

    let Subcommand::Dev(DevCommand {
        interactive: dev_cli,
    }) = subcommand.expect("dev present")
    else {
        unreachable!()
    };

    finalize_dev_interactive(interactive, root_overrides, dev_cli)
}

#[test]
fn root_command_defaults_to_workspace_launch() {
    let cli = MultitoolCli::try_parse_from(["praxis"]).expect("parse");
    assert!(!cli.interactive.is_dev_single_thread());
}

#[test]
fn dev_command_forces_single_thread_launch() {
    let interactive = finalize_dev_from_args(["praxis", "dev"].as_ref());
    assert!(interactive.is_dev_single_thread());
}

#[test]
fn dev_command_merges_scoped_interactive_flags() {
    let interactive = finalize_dev_from_args(
        [
            "praxis",
            "-c",
            "model_provider=deepseek",
            "dev",
            "-m",
            "deepseek-v4-pro",
            "inspect this",
        ]
        .as_ref(),
    );
    assert!(interactive.is_dev_single_thread());
    assert_eq!(interactive.model.as_deref(), Some("deepseek-v4-pro"));
    assert_eq!(interactive.prompt.as_deref(), Some("inspect this"));
    assert_eq!(
        interactive.config_overrides.raw_overrides,
        vec!["model_provider=deepseek".to_string()]
    );
}

#[test]
fn exec_resume_last_accepts_prompt_positional() {
    let cli = MultitoolCli::try_parse_from(["praxis", "exec", "--json", "resume", "--last", "2+2"])
        .expect("parse should succeed");

    let Some(Subcommand::Exec(exec)) = cli.subcommand else {
        panic!("expected exec subcommand");
    };
    let Some(praxis_exec::Command::Resume(args)) = exec.command else {
        panic!("expected exec resume");
    };

    assert!(args.last);
    assert_eq!(args.session_id, None);
    assert_eq!(args.prompt.as_deref(), Some("2+2"));
}

#[test]
fn exec_resume_accepts_output_last_message_flag_after_subcommand() {
    let cli = MultitoolCli::try_parse_from([
        "codex",
        "exec",
        "resume",
        "session-123",
        "-o",
        "/tmp/resume-output.md",
        "re-review",
    ])
    .expect("parse should succeed");

    let Some(Subcommand::Exec(exec)) = cli.subcommand else {
        panic!("expected exec subcommand");
    };
    let Some(praxis_exec::Command::Resume(args)) = exec.command else {
        panic!("expected exec resume");
    };

    assert_eq!(
        exec.last_message_file,
        Some(std::path::PathBuf::from("/tmp/resume-output.md"))
    );
    assert_eq!(args.session_id.as_deref(), Some("session-123"));
    assert_eq!(args.prompt.as_deref(), Some("re-review"));
}

fn app_gateway_from_args(args: &[&str]) -> AppGatewayCommand {
    let cli = MultitoolCli::try_parse_from(args).expect("parse");
    let Subcommand::AppGateway(app_gateway) = cli.subcommand.expect("app-gateway present") else {
        unreachable!()
    };
    app_gateway
}

fn sample_exit_info(conversation_id: Option<&str>, thread_name: Option<&str>) -> AppExitInfo {
    let token_usage = TokenUsage {
        output_tokens: 2,
        total_tokens: 2,
        ..Default::default()
    };
    AppExitInfo {
        token_usage,
        thread_id: conversation_id
            .map(ThreadId::from_string)
            .map(Result::unwrap),
        thread_name: thread_name.map(str::to_string),
        update_action: None,
        exit_reason: ExitReason::UserRequested,
    }
}

#[test]
fn format_exit_messages_skips_zero_usage() {
    let exit_info = AppExitInfo {
        token_usage: TokenUsage::default(),
        thread_id: None,
        thread_name: None,
        update_action: None,
        exit_reason: ExitReason::UserRequested,
    };
    let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
    assert!(lines.is_empty());
}

#[test]
fn format_exit_messages_includes_resume_hint_without_color() {
    let exit_info = sample_exit_info(
        Some("123e4567-e89b-12d3-a456-426614174000"),
        /*thread_name*/ None,
    );
    let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
    assert_eq!(
        lines,
        vec![
            "Token usage: total=2 input=0 output=2".to_string(),
            "To continue this session, run praxis resume 123e4567-e89b-12d3-a456-426614174000"
                .to_string(),
        ]
    );
}

#[test]
fn format_exit_messages_applies_color_when_enabled() {
    let exit_info = sample_exit_info(
        Some("123e4567-e89b-12d3-a456-426614174000"),
        /*thread_name*/ None,
    );
    let lines = format_exit_messages(exit_info, /*color_enabled*/ true);
    assert_eq!(lines.len(), 2);
    assert!(lines[1].contains("\u{1b}[36m"));
}

#[test]
fn format_exit_messages_prefers_thread_name() {
    let exit_info = sample_exit_info(
        Some("123e4567-e89b-12d3-a456-426614174000"),
        Some("my-thread"),
    );
    let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
    assert_eq!(
        lines,
        vec![
            "Token usage: total=2 input=0 output=2".to_string(),
            "To continue this session, run praxis resume my-thread".to_string(),
        ]
    );
}

#[test]
fn resume_model_flag_applies_when_no_root_flags() {
    let interactive =
        finalize_resume_from_args(["praxis", "resume", "-m", "gpt-5.1-test"].as_ref());

    assert_eq!(interactive.model.as_deref(), Some("gpt-5.1-test"));
    assert!(interactive.resume_picker);
    assert!(!interactive.resume_last);
    assert_eq!(interactive.resume_session_id, None);
    assert_eq!(interactive.resume_source, SessionLookupSource::Praxis);
}

#[test]
fn resume_picker_logic_none_and_not_last() {
    let interactive = finalize_resume_from_args(["praxis", "resume"].as_ref());
    assert!(interactive.resume_picker);
    assert!(!interactive.resume_last);
    assert_eq!(interactive.resume_session_id, None);
    assert!(!interactive.resume_show_all);
    assert_eq!(interactive.resume_source, SessionLookupSource::Praxis);
}

#[test]
fn resume_picker_logic_last() {
    let interactive = finalize_resume_from_args(["praxis", "resume", "--last"].as_ref());
    assert!(!interactive.resume_picker);
    assert!(interactive.resume_last);
    assert_eq!(interactive.resume_session_id, None);
    assert!(!interactive.resume_show_all);
    assert_eq!(interactive.resume_source, SessionLookupSource::Praxis);
}

#[test]
fn resume_picker_logic_with_session_id() {
    let interactive = finalize_resume_from_args(["praxis", "resume", "1234"].as_ref());
    assert!(!interactive.resume_picker);
    assert!(!interactive.resume_last);
    assert_eq!(interactive.resume_session_id.as_deref(), Some("1234"));
    assert!(!interactive.resume_show_all);
    assert_eq!(interactive.resume_source, SessionLookupSource::Praxis);
}

#[test]
fn resume_codex_without_session_id_opens_codex_picker() {
    let interactive = finalize_resume_from_args(["praxis", "resume", "codex"].as_ref());
    assert!(interactive.resume_picker);
    assert_eq!(interactive.resume_source, SessionLookupSource::Codex);
    assert_eq!(interactive.resume_session_id, None);
}

#[test]
fn resume_codex_with_session_id_targets_codex_lookup() {
    let interactive = finalize_resume_from_args(["praxis", "resume", "codex", "1234"].as_ref());
    assert!(!interactive.resume_picker);
    assert_eq!(interactive.resume_source, SessionLookupSource::Codex);
    assert_eq!(interactive.resume_session_id.as_deref(), Some("1234"));
}

#[test]
fn resume_codex_last_keeps_codex_lookup_source() {
    let cli = MultitoolCli::try_parse_from(["praxis", "resume", "codex", "--last"]).expect("parse");
    let Subcommand::Resume(ResumeCommand {
        target,
        target_extra,
        last,
        ..
    }) = cli.subcommand.expect("resume present")
    else {
        unreachable!()
    };
    let targets = collect_session_target_args(target, target_extra);
    let parsed_target = parse_session_target_args(targets, "resume").expect("parse target");
    validate_session_target_with_last(&parsed_target, last, "resume").expect("validate");
    assert!(last);
    assert_eq!(
        parsed_target.source.lookup_source(),
        SessionLookupSource::Codex
    );
    assert_eq!(parsed_target.session_id, None);
}

#[test]
fn resume_all_flag_sets_show_all() {
    let interactive = finalize_resume_from_args(["praxis", "resume", "--all"].as_ref());
    assert!(interactive.resume_picker);
    assert!(interactive.resume_show_all);
    assert_eq!(interactive.resume_source, SessionLookupSource::Praxis);
}

#[test]
fn resume_include_non_interactive_flag_sets_source_filter_override() {
    let interactive =
        finalize_resume_from_args(["praxis", "resume", "--include-non-interactive"].as_ref());

    assert!(interactive.resume_picker);
    assert!(interactive.resume_include_non_interactive);
    assert_eq!(interactive.resume_source, SessionLookupSource::Praxis);
}

#[test]
fn resume_merges_option_flags_and_full_auto() {
    let interactive = finalize_resume_from_args(
        [
            "codex",
            "resume",
            "sid",
            "--oss",
            "--full-auto",
            "--search",
            "--sandbox",
            "workspace-write",
            "--ask-for-approval",
            "on-request",
            "-m",
            "gpt-5.1-test",
            "-p",
            "my-profile",
            "-C",
            "/tmp",
            "-i",
            "/tmp/a.png,/tmp/b.png",
        ]
        .as_ref(),
    );

    assert_eq!(interactive.model.as_deref(), Some("gpt-5.1-test"));
    assert!(interactive.oss);
    assert_eq!(interactive.config_profile.as_deref(), Some("my-profile"));
    assert_matches!(
        interactive.sandbox_mode,
        Some(praxis_utils_cli::SandboxModeCliArg::WorkspaceWrite)
    );
    assert_matches!(
        interactive.approval_policy,
        Some(praxis_utils_cli::ApprovalModeCliArg::OnRequest)
    );
    assert!(interactive.full_auto);
    assert_eq!(
        interactive.cwd.as_deref(),
        Some(std::path::Path::new("/tmp"))
    );
    assert!(interactive.web_search);
    let has_a = interactive
        .images
        .iter()
        .any(|p| p == std::path::Path::new("/tmp/a.png"));
    let has_b = interactive
        .images
        .iter()
        .any(|p| p == std::path::Path::new("/tmp/b.png"));
    assert!(has_a && has_b);
    assert!(!interactive.resume_picker);
    assert!(!interactive.resume_last);
    assert_eq!(interactive.resume_session_id.as_deref(), Some("sid"));
    assert_eq!(interactive.resume_source, SessionLookupSource::Praxis);
}

#[test]
fn resume_merges_dangerously_bypass_flag() {
    let interactive = finalize_resume_from_args(
        [
            "codex",
            "resume",
            "--dangerously-bypass-approvals-and-sandbox",
        ]
        .as_ref(),
    );
    assert!(interactive.dangerously_bypass_approvals_and_sandbox);
    assert!(interactive.resume_picker);
    assert!(!interactive.resume_last);
    assert_eq!(interactive.resume_session_id, None);
    assert_eq!(interactive.resume_source, SessionLookupSource::Praxis);
}

#[test]
fn fork_picker_logic_none_and_not_last() {
    let interactive = finalize_fork_from_args(["praxis", "fork"].as_ref());
    assert!(interactive.fork_picker);
    assert!(!interactive.fork_last);
    assert_eq!(interactive.fork_session_id, None);
    assert!(!interactive.fork_show_all);
    assert_eq!(interactive.fork_source, SessionLookupSource::Praxis);
}

#[test]
fn fork_picker_logic_last() {
    let interactive = finalize_fork_from_args(["praxis", "fork", "--last"].as_ref());
    assert!(!interactive.fork_picker);
    assert!(interactive.fork_last);
    assert_eq!(interactive.fork_session_id, None);
    assert!(!interactive.fork_show_all);
    assert_eq!(interactive.fork_source, SessionLookupSource::Praxis);
}

#[test]
fn fork_picker_logic_with_session_id() {
    let interactive = finalize_fork_from_args(["praxis", "fork", "1234"].as_ref());
    assert!(!interactive.fork_picker);
    assert!(!interactive.fork_last);
    assert_eq!(interactive.fork_session_id.as_deref(), Some("1234"));
    assert!(!interactive.fork_show_all);
    assert_eq!(interactive.fork_source, SessionLookupSource::Praxis);
}

#[test]
fn fork_codex_without_session_id_opens_codex_picker() {
    let interactive = finalize_fork_from_args(["praxis", "fork", "codex"].as_ref());
    assert!(interactive.fork_picker);
    assert_eq!(interactive.fork_source, SessionLookupSource::Codex);
    assert_eq!(interactive.fork_session_id, None);
}

#[test]
fn resume_last_conflicts_with_explicit_session_target() {
    let parsed_target =
        parse_session_target_args(vec!["sid".to_string()], "resume").expect("parse target");
    let err = validate_session_target_with_last(&parsed_target, /*last*/ true, "resume")
        .expect_err("validation should fail");
    assert!(
        err.to_string()
            .contains("cannot be combined with an explicit session id")
    );
}

#[test]
fn fork_all_flag_sets_show_all() {
    let interactive = finalize_fork_from_args(["praxis", "fork", "--all"].as_ref());
    assert!(interactive.fork_picker);
    assert!(interactive.fork_show_all);
    assert_eq!(interactive.fork_source, SessionLookupSource::Praxis);
}

#[test]
fn app_gateway_analytics_default_disabled_without_flag() {
    let app_gateway = app_gateway_from_args(["praxis", "app-gateway"].as_ref());
    assert!(!app_gateway.analytics_default_enabled);
    assert_eq!(
        app_gateway.listen,
        praxis_app_gateway::AppGatewayTransport::Stdio
    );
}

#[test]
fn app_gateway_analytics_default_enabled_with_flag() {
    let app_gateway =
        app_gateway_from_args(["praxis", "app-gateway", "--analytics-default-enabled"].as_ref());
    assert!(app_gateway.analytics_default_enabled);
}

#[test]
fn remote_flag_parses_for_interactive_root() {
    let cli =
        MultitoolCli::try_parse_from(["praxis", "--remote", "ws://127.0.0.1:4500"]).expect("parse");
    assert_eq!(cli.remote.remote.as_deref(), Some("ws://127.0.0.1:4500"));
}

#[test]
fn control_listen_flag_parses_for_interactive_root() {
    let cli = MultitoolCli::try_parse_from(["praxis", "--control-listen", "ws://127.0.0.1:4222"])
        .expect("parse");
    assert_eq!(
        cli.remote.control_listen.as_deref(),
        Some("ws://127.0.0.1:4222")
    );
}

#[test]
fn no_control_listen_flag_parses_for_interactive_root() {
    let cli = MultitoolCli::try_parse_from(["praxis", "--no-control-listen"]).expect("parse");
    assert!(cli.remote.no_control_listen);
}

#[test]
fn remote_auth_token_env_flag_parses_for_interactive_root() {
    let cli = MultitoolCli::try_parse_from([
        "codex",
        "--remote-auth-token-env",
        "PRAXIS_REMOTE_AUTH_TOKEN",
        "--remote",
        "ws://127.0.0.1:4500",
    ])
    .expect("parse");
    assert_eq!(
        cli.remote.remote_auth_token_env.as_deref(),
        Some("PRAXIS_REMOTE_AUTH_TOKEN")
    );
}

#[test]
fn remote_flag_parses_for_resume_subcommand() {
    let cli = MultitoolCli::try_parse_from(["praxis", "resume", "--remote", "ws://127.0.0.1:4500"])
        .expect("parse");
    let Subcommand::Resume(ResumeCommand { remote, .. }) = cli.subcommand.expect("resume present")
    else {
        panic!("expected resume subcommand");
    };
    assert_eq!(remote.remote.as_deref(), Some("ws://127.0.0.1:4500"));
}

#[test]
fn reject_remote_mode_for_other_non_interactive_subcommands() {
    let err = reject_remote_mode_for_subcommand(
        Some("127.0.0.1:4500"),
        /*remote_auth_token_env*/ None,
        "mcp-server",
    )
    .expect_err("non-interactive subcommands should reject --remote");
    assert!(
        err.to_string()
            .contains("only supported for interactive TUI commands")
    );
}

#[test]
fn reject_remote_auth_token_env_for_non_interactive_subcommands() {
    let err = reject_remote_mode_for_subcommand(
        /*remote*/ None,
        Some("PRAXIS_REMOTE_AUTH_TOKEN"),
        "mcp-server",
    )
    .expect_err("non-interactive subcommands should reject --remote-auth-token-env");
    assert!(
        err.to_string()
            .contains("only supported for interactive TUI commands")
    );
}

#[test]
fn reject_control_options_for_non_interactive_subcommands() {
    let cli = MultitoolCli::try_parse_from([
        "codex",
        "--control-listen",
        "ws://127.0.0.1:4222",
        "mcp-server",
    ])
    .expect("parse");
    let err = reject_control_options_for_noninteractive_subcommand(
        cli.remote.control_listen.as_deref(),
        cli.remote.no_control_listen,
        cli.subcommand.as_ref(),
    )
    .expect_err("non-interactive subcommands should reject --control-listen");
    assert!(
        err.to_string()
            .contains("only supported for Praxis Center/TUI commands")
    );
}

#[test]
fn merge_control_listen_options_prefers_command_explicit_addr() {
    let (addr, disabled) = merge_control_listen_options(
        None,
        /*root_no_control_listen*/ true,
        Some("ws://127.0.0.1:4223".to_string()),
        /*command_no_control_listen*/ false,
    );
    assert_eq!(addr.as_deref(), Some("ws://127.0.0.1:4223"));
    assert!(!disabled);
}

#[test]
fn reject_remote_auth_token_env_for_app_gateway_generate_internal_json_schema() {
    let subcommand =
        AppGatewaySubcommand::GenerateInternalJsonSchema(GenerateInternalJsonSchemaCommand {
            out_dir: PathBuf::from("/tmp/out"),
        });
    let err = reject_remote_mode_for_app_gateway_subcommand(
        /*remote*/ None,
        Some("PRAXIS_REMOTE_AUTH_TOKEN"),
        Some(&subcommand),
    )
    .expect_err("non-interactive app-gateway subcommands should reject --remote-auth-token-env");
    assert!(err.to_string().contains("generate-internal-json-schema"));
}

#[test]
fn read_remote_auth_token_from_env_var_reports_missing_values() {
    let err = read_remote_auth_token_from_env_var_with("PRAXIS_REMOTE_AUTH_TOKEN", |_| {
        Err(std::env::VarError::NotPresent)
    })
    .expect_err("missing env vars should be rejected");
    assert!(err.to_string().contains("is not set"));
}

#[test]
fn read_remote_auth_token_from_env_var_trims_values() {
    let auth_token = read_remote_auth_token_from_env_var_with("PRAXIS_REMOTE_AUTH_TOKEN", |_| {
        Ok("  bearer-token  ".to_string())
    })
    .expect("env var should parse");
    assert_eq!(auth_token, "bearer-token");
}

#[test]
fn read_remote_auth_token_from_env_var_rejects_empty_values() {
    let err = read_remote_auth_token_from_env_var_with("PRAXIS_REMOTE_AUTH_TOKEN", |_| {
        Ok(" \n\t ".to_string())
    })
    .expect_err("empty env vars should be rejected");
    assert!(err.to_string().contains("is empty"));
}

#[test]
fn app_gateway_listen_websocket_url_parses() {
    let app_gateway = app_gateway_from_args(
        ["praxis", "app-gateway", "--listen", "ws://127.0.0.1:4500"].as_ref(),
    );
    assert_eq!(
        app_gateway.listen,
        praxis_app_gateway::AppGatewayTransport::WebSocket {
            bind_address: "127.0.0.1:4500".parse().expect("valid socket address"),
        }
    );
}

#[test]
fn app_gateway_listen_stdio_url_parses() {
    let app_gateway =
        app_gateway_from_args(["praxis", "app-gateway", "--listen", "stdio://"].as_ref());
    assert_eq!(
        app_gateway.listen,
        praxis_app_gateway::AppGatewayTransport::Stdio
    );
}

#[test]
fn app_gateway_listen_invalid_url_fails_to_parse() {
    let parse_result =
        MultitoolCli::try_parse_from(["praxis", "app-gateway", "--listen", "http://foo"]);
    assert!(parse_result.is_err());
}

#[test]
fn app_gateway_capability_token_flags_parse() {
    let app_gateway = app_gateway_from_args(
        [
            "codex",
            "app-gateway",
            "--ws-auth",
            "capability-token",
            "--ws-token-file",
            "/tmp/praxis-token",
        ]
        .as_ref(),
    );
    assert_eq!(
        app_gateway.auth.ws_auth,
        Some(praxis_app_gateway::WebsocketAuthCliMode::CapabilityToken)
    );
    assert_eq!(
        app_gateway.auth.ws_token_file,
        Some(PathBuf::from("/tmp/praxis-token"))
    );
}

#[test]
fn app_gateway_signed_bearer_flags_parse() {
    let app_gateway = app_gateway_from_args(
        [
            "codex",
            "app-gateway",
            "--ws-auth",
            "signed-bearer-token",
            "--ws-shared-secret-file",
            "/tmp/praxis-secret",
            "--ws-issuer",
            "issuer",
            "--ws-audience",
            "audience",
            "--ws-max-clock-skew-seconds",
            "9",
        ]
        .as_ref(),
    );
    assert_eq!(
        app_gateway.auth.ws_auth,
        Some(praxis_app_gateway::WebsocketAuthCliMode::SignedBearerToken)
    );
    assert_eq!(
        app_gateway.auth.ws_shared_secret_file,
        Some(PathBuf::from("/tmp/praxis-secret"))
    );
    assert_eq!(app_gateway.auth.ws_issuer.as_deref(), Some("issuer"));
    assert_eq!(app_gateway.auth.ws_audience.as_deref(), Some("audience"));
    assert_eq!(app_gateway.auth.ws_max_clock_skew_seconds, Some(9));
}

#[test]
fn app_gateway_rejects_removed_insecure_non_loopback_flag() {
    let parse_result = MultitoolCli::try_parse_from([
        "codex",
        "app-gateway",
        "--allow-unauthenticated-non-loopback-ws",
    ]);
    assert!(parse_result.is_err());
}

#[test]
fn features_enable_parses_feature_name() {
    let cli = MultitoolCli::try_parse_from(["praxis", "features", "enable", "unified_exec"])
        .expect("parse should succeed");
    let Some(Subcommand::Features(FeaturesCli { sub })) = cli.subcommand else {
        panic!("expected features subcommand");
    };
    let FeaturesSubcommand::Enable(FeatureSetArgs { feature }) = sub else {
        panic!("expected features enable");
    };
    assert_eq!(feature, "unified_exec");
}

#[test]
fn features_disable_parses_feature_name() {
    let cli = MultitoolCli::try_parse_from(["praxis", "features", "disable", "shell_tool"])
        .expect("parse should succeed");
    let Some(Subcommand::Features(FeaturesCli { sub })) = cli.subcommand else {
        panic!("expected features subcommand");
    };
    let FeaturesSubcommand::Disable(FeatureSetArgs { feature }) = sub else {
        panic!("expected features disable");
    };
    assert_eq!(feature, "shell_tool");
}

#[test]
fn feature_toggles_known_features_generate_overrides() {
    let toggles = FeatureToggles {
        enable: vec!["web_search_request".to_string()],
        disable: vec!["unified_exec".to_string()],
    };
    let overrides = toggles.to_overrides().expect("valid features");
    assert_eq!(
        overrides,
        vec![
            "features.web_search_request=true".to_string(),
            "features.unified_exec=false".to_string(),
        ]
    );
}

#[test]
fn feature_toggles_accept_legacy_linux_sandbox_flag() {
    let toggles = FeatureToggles {
        enable: vec!["use_linux_sandbox_bwrap".to_string()],
        disable: Vec::new(),
    };
    let overrides = toggles.to_overrides().expect("valid features");
    assert_eq!(
        overrides,
        vec!["features.use_linux_sandbox_bwrap=true".to_string(),]
    );
}

#[test]
fn feature_toggles_unknown_feature_errors() {
    let toggles = FeatureToggles {
        enable: vec!["does_not_exist".to_string()],
        disable: Vec::new(),
    };
    let err = toggles
        .to_overrides()
        .expect_err("feature should be rejected");
    assert_eq!(err.to_string(), "Unknown feature flag: does_not_exist");
}
