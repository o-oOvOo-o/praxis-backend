use clap::CommandFactory;
use clap::Parser;
use clap_complete::generate;
use praxis_app_gateway_service as praxis_app_gateway;
use praxis_arg0::Arg0DispatchPaths;
use praxis_arg0::arg0_dispatch_or_else;
use praxis_chatgpt::apply_command::run_apply_command;
use praxis_cli::login::read_api_key_from_stdin;
use praxis_cli::login::run_login_status;
use praxis_cli::login::run_login_with_api_key;
use praxis_cli::login::run_login_with_chatgpt;
use praxis_cli::login::run_login_with_device_code;
use praxis_cli::login::run_logout;
use praxis_exec::Cli as ExecCli;
use praxis_exec::Command as ExecCommand;
use praxis_execpolicy::ExecPolicyCheckCommand;

#[cfg(target_os = "macos")]
mod app_cmd;
mod commands;
mod debug_commands;
#[cfg(target_os = "macos")]
mod desktop_app;
mod dispatch_support;
mod exit_handling;
mod feature_flags;
mod interactive_launch;
mod mcp_cmd;
mod remote_control;
mod session_target;
#[cfg(not(windows))]
mod wsl_paths;

use crate::commands::*;
use crate::debug_commands::*;
use crate::dispatch_support::prepend_config_flags;
use crate::exit_handling::handle_app_exit;
use crate::feature_flags::*;
use crate::interactive_launch::*;
use crate::remote_control::*;
use crate::session_target::*;
use praxis_core::config::Config;
use praxis_core::config::ConfigOverrides;
use praxis_core::util::PRIMARY_CLI_COMMAND;
use praxis_features::FEATURES;

fn run_execpolicycheck(cmd: ExecPolicyCheckCommand) -> anyhow::Result<()> {
    cmd.run()
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        cli_main(arg0_paths).await?;
        Ok(())
    })
}

async fn cli_main(arg0_paths: Arg0DispatchPaths) -> anyhow::Result<()> {
    let MultitoolCli {
        config_overrides: mut root_config_overrides,
        feature_toggles,
        remote,
        mut interactive,
        subcommand,
    } = MultitoolCli::parse();

    // Fold --enable/--disable into config overrides so they flow to all subcommands.
    let toggle_overrides = feature_toggles.to_overrides()?;
    root_config_overrides.raw_overrides.extend(toggle_overrides);
    let root_remote = remote.remote;
    let root_control_listen = remote.control_listen;
    let root_no_control_listen = remote.no_control_listen;
    let root_remote_auth_token_env = remote.remote_auth_token_env;
    reject_control_options_for_noninteractive_subcommand(
        root_control_listen.as_deref(),
        root_no_control_listen,
        subcommand.as_ref(),
    )?;

    match subcommand {
        None => {
            prepend_config_flags(
                &mut interactive.config_overrides,
                root_config_overrides.clone(),
            );
            let exit_info = run_interactive_tui(
                interactive,
                root_remote.clone(),
                root_control_listen.clone(),
                root_no_control_listen,
                root_remote_auth_token_env.clone(),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Exec(mut exec_cli)) => {
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            let remote_app_gateway = resolve_exec_remote_app_gateway(
                root_remote.clone(),
                root_remote_auth_token_env.clone(),
            )
            .await?;
            praxis_exec::run_main(exec_cli, arg0_paths.clone(), remote_app_gateway).await?;
        }
        Some(Subcommand::Review(review_args)) => {
            let mut exec_cli = ExecCli::try_parse_from(["praxis", "exec"])?;
            exec_cli.command = Some(ExecCommand::Review(review_args));
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            let remote_app_gateway = resolve_exec_remote_app_gateway(
                root_remote.clone(),
                root_remote_auth_token_env.clone(),
            )
            .await?;
            praxis_exec::run_main(exec_cli, arg0_paths.clone(), remote_app_gateway).await?;
        }
        Some(Subcommand::McpServer) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "mcp-server",
            )?;
            praxis_mcp_server::run_main(arg0_paths.clone(), root_config_overrides).await?;
        }
        Some(Subcommand::Mcp(mut mcp_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "mcp",
            )?;
            // Propagate any root-level config overrides (e.g. `-c key=value`).
            prepend_config_flags(&mut mcp_cli.config_overrides, root_config_overrides.clone());
            mcp_cli.run().await?;
        }
        Some(Subcommand::AppGateway(app_gateway_cli)) => {
            let AppGatewayCommand {
                subcommand,
                listen,
                analytics_default_enabled,
                auth,
            } = app_gateway_cli;
            reject_remote_mode_for_app_gateway_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                subcommand.as_ref(),
            )?;
            match subcommand {
                None => {
                    let transport = listen;
                    let auth = auth.try_into_settings()?;
                    praxis_app_gateway::run_service_gateway(
                        praxis_app_gateway::ServiceGatewayStartArgs {
                            arg0_paths: arg0_paths.clone(),
                            cli_config_overrides: root_config_overrides,
                            loader_overrides: praxis_core::config_loader::LoaderOverrides::default(
                            ),
                            default_analytics_enabled: analytics_default_enabled,
                            listen: transport,
                            session_source: praxis_protocol::protocol::SessionSource::AppGateway,
                            auth,
                        },
                    )
                    .await?;
                }
                Some(AppGatewaySubcommand::GenerateTs(gen_cli)) => {
                    let options = praxis_app_gateway_protocol::GenerateTsOptions {
                        experimental_api: gen_cli.experimental,
                        ..Default::default()
                    };
                    praxis_app_gateway_protocol::generate_ts_with_options(
                        &gen_cli.out_dir,
                        gen_cli.prettier.as_deref(),
                        options,
                    )?;
                }
                Some(AppGatewaySubcommand::GenerateJsonSchema(gen_cli)) => {
                    praxis_app_gateway_protocol::generate_json_with_experimental(
                        &gen_cli.out_dir,
                        gen_cli.experimental,
                    )?;
                }
                Some(AppGatewaySubcommand::GenerateInternalJsonSchema(gen_cli)) => {
                    praxis_app_gateway_protocol::generate_internal_json_schema(&gen_cli.out_dir)?;
                }
            }
        }
        Some(Subcommand::Dev(DevCommand {
            interactive: dev_interactive,
        })) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "dev",
            )?;
            interactive = finalize_dev_interactive(
                interactive,
                root_config_overrides.clone(),
                dev_interactive,
            );
            let exit_info =
                run_interactive_tui(interactive, None, None, true, None, arg0_paths.clone())
                    .await?;
            handle_app_exit(exit_info)?;
        }
        #[cfg(target_os = "macos")]
        Some(Subcommand::App(app_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "app",
            )?;
            app_cmd::run_app(app_cli).await?;
        }
        Some(Subcommand::Resume(ResumeCommand {
            target,
            target_extra,
            last,
            all,
            include_non_interactive,
            remote,
            config_overrides,
        })) => {
            let targets = collect_session_target_args(target, target_extra);
            let parsed_target = parse_session_target_args(targets, "resume")?;
            validate_session_target_with_last(&parsed_target, last, "resume")?;
            interactive = finalize_resume_interactive(
                interactive,
                root_config_overrides.clone(),
                parsed_target.session_id,
                parsed_target.source.lookup_source(),
                last,
                all,
                include_non_interactive,
                config_overrides,
            );
            let (control_listen, no_control_listen) = merge_control_listen_options(
                root_control_listen.clone(),
                root_no_control_listen,
                remote.control_listen,
                remote.no_control_listen,
            );
            let exit_info = run_interactive_tui(
                interactive,
                remote.remote.or(root_remote.clone()),
                control_listen,
                no_control_listen,
                remote
                    .remote_auth_token_env
                    .or(root_remote_auth_token_env.clone()),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Fork(ForkCommand {
            target,
            target_extra,
            last,
            all,
            remote,
            config_overrides,
        })) => {
            let targets = collect_session_target_args(target, target_extra);
            let parsed_target = parse_session_target_args(targets, "fork")?;
            validate_session_target_with_last(&parsed_target, last, "fork")?;
            interactive = finalize_fork_interactive(
                interactive,
                root_config_overrides.clone(),
                parsed_target.session_id,
                parsed_target.source.lookup_source(),
                last,
                all,
                config_overrides,
            );
            let (control_listen, no_control_listen) = merge_control_listen_options(
                root_control_listen.clone(),
                root_no_control_listen,
                remote.control_listen,
                remote.no_control_listen,
            );
            let exit_info = run_interactive_tui(
                interactive,
                remote.remote.or(root_remote.clone()),
                control_listen,
                no_control_listen,
                remote
                    .remote_auth_token_env
                    .or(root_remote_auth_token_env.clone()),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Login(mut login_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "login",
            )?;
            prepend_config_flags(
                &mut login_cli.config_overrides,
                root_config_overrides.clone(),
            );
            match login_cli.action {
                Some(LoginSubcommand::Status) => {
                    run_login_status(login_cli.config_overrides).await;
                }
                None => {
                    if login_cli.use_device_code {
                        run_login_with_device_code(
                            login_cli.config_overrides,
                            login_cli.issuer_base_url,
                            login_cli.client_id,
                        )
                        .await;
                    } else if login_cli.api_key.is_some() {
                        eprintln!(
                            "The --api-key flag is no longer supported. Pipe the key instead, e.g. `printenv OPENAI_API_KEY | praxis login --with-api-key`."
                        );
                        std::process::exit(1);
                    } else if login_cli.with_api_key {
                        let api_key = read_api_key_from_stdin();
                        run_login_with_api_key(login_cli.config_overrides, api_key).await;
                    } else {
                        run_login_with_chatgpt(login_cli.config_overrides).await;
                    }
                }
            }
        }
        Some(Subcommand::Logout(mut logout_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "logout",
            )?;
            prepend_config_flags(
                &mut logout_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_logout(logout_cli.config_overrides).await;
        }
        Some(Subcommand::Completion(completion_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "completion",
            )?;
            print_completion(completion_cli);
        }
        Some(Subcommand::Cloud(mut cloud_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "cloud",
            )?;
            prepend_config_flags(
                &mut cloud_cli.config_overrides,
                root_config_overrides.clone(),
            );
            praxis_cloud_tasks::run_main(cloud_cli, arg0_paths.praxis_linux_sandbox_exe.clone())
                .await?;
        }
        Some(Subcommand::Sandbox(sandbox_args)) => match sandbox_args.cmd {
            SandboxCommand::Macos(mut seatbelt_cli) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "sandbox macos",
                )?;
                prepend_config_flags(
                    &mut seatbelt_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                praxis_cli::debug_sandbox::run_command_under_seatbelt(
                    seatbelt_cli,
                    arg0_paths.praxis_linux_sandbox_exe.clone(),
                )
                .await?;
            }
            SandboxCommand::Linux(mut landlock_cli) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "sandbox linux",
                )?;
                prepend_config_flags(
                    &mut landlock_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                praxis_cli::debug_sandbox::run_command_under_landlock(
                    landlock_cli,
                    arg0_paths.praxis_linux_sandbox_exe.clone(),
                )
                .await?;
            }
            SandboxCommand::Windows(mut windows_cli) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "sandbox windows",
                )?;
                prepend_config_flags(
                    &mut windows_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                praxis_cli::debug_sandbox::run_command_under_windows(
                    windows_cli,
                    arg0_paths.praxis_linux_sandbox_exe.clone(),
                )
                .await?;
            }
        },
        Some(Subcommand::Debug(DebugCommand { subcommand })) => match subcommand {
            DebugSubcommand::AppGateway(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug app-gateway",
                )?;
                run_debug_app_gateway_command(cmd).await?;
            }
            DebugSubcommand::WebSearch(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug web-search",
                )?;
                run_debug_web_search_command(cmd).await?;
            }
            DebugSubcommand::ClearMemories => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug clear-memories",
                )?;
                run_debug_clear_memories_command(&root_config_overrides, &interactive).await?;
            }
        },
        Some(Subcommand::Execpolicy(ExecpolicyCommand { sub })) => match sub {
            ExecpolicySubcommand::Check(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "execpolicy check",
                )?;
                run_execpolicycheck(cmd)?
            }
        },
        Some(Subcommand::Apply(mut apply_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "apply",
            )?;
            prepend_config_flags(
                &mut apply_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_apply_command(apply_cli, /*cwd*/ None).await?;
        }
        Some(Subcommand::ResponsesApiProxy(args)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "responses-api-proxy",
            )?;
            tokio::task::spawn_blocking(move || praxis_responses_api_proxy::run_main(args))
                .await??;
        }
        Some(Subcommand::StdioToUds(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "stdio-to-uds",
            )?;
            let socket_path = cmd.socket_path;
            tokio::task::spawn_blocking(move || praxis_stdio_to_uds::run(socket_path.as_path()))
                .await??;
        }
        Some(Subcommand::Features(FeaturesCli { sub })) => match sub {
            FeaturesSubcommand::List => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features list",
                )?;
                // Respect root-level `-c` overrides plus top-level flags like `--profile`.
                let mut cli_kv_overrides = root_config_overrides
                    .parse_overrides()
                    .map_err(anyhow::Error::msg)?;

                // Honor `--search` via the canonical web_search mode.
                if interactive.web_search {
                    cli_kv_overrides.push((
                        "web_search".to_string(),
                        toml::Value::String("live".to_string()),
                    ));
                }

                // Thread through relevant top-level flags (at minimum, `--profile`).
                let overrides = ConfigOverrides {
                    config_profile: interactive.config_profile.clone(),
                    ..Default::default()
                };

                let config = Config::load_with_cli_overrides_and_harness_overrides(
                    cli_kv_overrides,
                    overrides,
                )
                .await?;
                let mut rows = Vec::with_capacity(FEATURES.len());
                let mut name_width = 0;
                let mut stage_width = 0;
                for def in FEATURES {
                    let name = def.key;
                    let stage = stage_str(def.stage);
                    let enabled = config.features.enabled(def.id);
                    name_width = name_width.max(name.len());
                    stage_width = stage_width.max(stage.len());
                    rows.push((name, stage, enabled));
                }
                rows.sort_unstable_by_key(|(name, _, _)| *name);

                for (name, stage, enabled) in rows {
                    println!("{name:<name_width$}  {stage:<stage_width$}  {enabled}");
                }
            }
            FeaturesSubcommand::Enable(FeatureSetArgs { feature }) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features enable",
                )?;
                enable_feature_in_config(&interactive, &feature).await?;
            }
            FeaturesSubcommand::Disable(FeatureSetArgs { feature }) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features disable",
                )?;
                disable_feature_in_config(&interactive, &feature).await?;
            }
        },
    }

    Ok(())
}

fn print_completion(cmd: CompletionCommand) {
    let mut app = MultitoolCli::command();
    let name = PRIMARY_CLI_COMMAND;
    generate(cmd.shell, &mut app, name, &mut std::io::stdout());
}

#[cfg(test)]
mod main_tests;
