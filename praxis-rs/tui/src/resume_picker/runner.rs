use super::*;

/// Interactive session picker that lists recorded threads with simple search and
/// pagination.
///
/// The picker displays sessions in a table with timestamp columns (created/updated),
/// git branch, working directory, and conversation preview. Users can toggle
/// between sorting by creation time and last-updated time using the Tab key.
///
/// Sessions are loaded on-demand via cursor-based pagination. App Gateway
/// returns pages ordered by the selected sort key, and the picker deduplicates
/// across pages to handle overlapping windows when new sessions appear during
/// pagination.
///
/// Filtering happens in two layers: thread source/search at App Gateway, then
/// optional working-directory filtering in the picker.

pub async fn run_resume_picker_with_app_gateway(
    tui: &mut Tui,
    config: &Config,
    show_all: bool,
    include_non_interactive: bool,
    active_source: SessionLookupSource,
    app_gateway: AppGatewaySession,
    alternate_source: Option<AlternatePickerSource>,
) -> Result<SessionSelection> {
    let (bg_tx, bg_rx) = mpsc::unbounded_channel();
    let is_remote = app_gateway.is_remote();
    let primary_loader =
        spawn_app_gateway_page_loader(app_gateway, include_non_interactive, bg_tx.clone());
    let source_switcher = alternate_source.map(|alternate| {
        let alternate_loader =
            spawn_app_gateway_page_loader(alternate.app_gateway, include_non_interactive, bg_tx);
        SourceSwitcher::from_sources(
            active_source,
            picker_source_config(config, primary_loader.clone()),
            alternate.source,
            picker_source_config(&alternate.config, alternate_loader),
        )
    });
    run_session_picker_with_loader(
        tui,
        config,
        show_all,
        SessionPickerAction::Resume,
        is_remote,
        primary_loader,
        bg_rx,
        active_source,
        source_switcher,
    )
    .await
}

pub async fn run_fork_picker_with_app_gateway(
    tui: &mut Tui,
    config: &Config,
    show_all: bool,
    active_source: SessionLookupSource,
    app_gateway: AppGatewaySession,
    alternate_source: Option<AlternatePickerSource>,
) -> Result<SessionSelection> {
    let (bg_tx, bg_rx) = mpsc::unbounded_channel();
    let is_remote = app_gateway.is_remote();
    let primary_loader = spawn_app_gateway_page_loader(
        app_gateway,
        /*include_non_interactive*/ false,
        bg_tx.clone(),
    );
    let source_switcher = alternate_source.map(|alternate| {
        let alternate_loader = spawn_app_gateway_page_loader(
            alternate.app_gateway,
            /*include_non_interactive*/ false,
            bg_tx,
        );
        SourceSwitcher::from_sources(
            active_source,
            picker_source_config(config, primary_loader.clone()),
            alternate.source,
            picker_source_config(&alternate.config, alternate_loader),
        )
    });
    run_session_picker_with_loader(
        tui,
        config,
        show_all,
        SessionPickerAction::Fork,
        is_remote,
        primary_loader,
        bg_rx,
        active_source,
        source_switcher,
    )
    .await
}

async fn run_session_picker_with_loader(
    tui: &mut Tui,
    config: &Config,
    show_all: bool,
    action: SessionPickerAction,
    is_remote: bool,
    page_loader: PageLoader,
    bg_rx: mpsc::UnboundedReceiver<BackgroundEvent>,
    active_source: SessionLookupSource,
    source_switcher: Option<SourceSwitcher>,
) -> Result<SessionSelection> {
    let alt = AltScreenGuard::enter(tui);
    let praxis_home = config.praxis_home.as_path();
    let filter_cwd = if show_all || is_remote {
        // Remote sessions live in the server's filesystem namespace, so the client
        // process cwd is not a meaningful default filter. A real remote cwd filter
        // would need an explicit server-side target cwd instead of current_dir().
        None
    } else {
        Some(config.cwd.as_path().to_path_buf())
    };

    let mut state = PickerState::new(
        praxis_home.to_path_buf(),
        alt.tui.frame_requester(),
        page_loader,
        show_all,
        filter_cwd,
        action,
    );
    state.set_active_source(active_source);
    if let Some(source_switcher) = source_switcher {
        state.configure_source_switcher(active_source, source_switcher);
    }
    state.start_initial_load();
    state.request_frame();

    let mut tui_events = alt.tui.event_stream().fuse();
    let mut background_events = UnboundedReceiverStream::new(bg_rx).fuse();

    loop {
        tokio::select! {
            Some(ev) = tui_events.next() => {
                match ev {
                    TuiEvent::Key(key) => {
                        if matches!(key.kind, KeyEventKind::Release) {
                            continue;
                        }
                        if let Some(sel) = state.handle_key(key).await? {
                            return Ok(sel);
                        }
                    }
                    TuiEvent::Draw => {
                        if let Ok(size) = alt.tui.terminal.size() {
                            state.update_view_rows(size.height.saturating_sub(4) as usize);
                        }
                        draw_picker(alt.tui, &state)?;
                    }
                    _ => {}
                }
            }
            Some(event) = background_events.next() => {
                state.handle_background_event(event).await?;
            }
            else => break,
        }
    }

    // Fallback – treat as cancel/new
    Ok(SessionSelection::StartFresh)
}

/// RAII guard that ensures we leave the alt-screen on scope exit.
struct AltScreenGuard<'a> {
    tui: &'a mut Tui,
}

impl<'a> AltScreenGuard<'a> {
    fn enter(tui: &'a mut Tui) -> Self {
        let _ = tui.enter_alt_screen();
        Self { tui }
    }
}

impl Drop for AltScreenGuard<'_> {
    fn drop(&mut self) {
        let _ = self.tui.leave_alt_screen();
    }
}
