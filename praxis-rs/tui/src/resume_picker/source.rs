use super::*;

#[derive(Clone)]
pub(super) struct PageLoadRequest {
    pub(super) cursor: Option<PageCursor>,
    pub(super) request_token: usize,
    pub(super) search_token: Option<usize>,
    pub(super) search_term: Option<String>,
    pub(super) filter_cwd: Option<PathBuf>,
    pub(super) sort_key: ThreadSortKey,
    pub(super) archive_filter: ThreadArchiveFilter,
}

pub(super) type PageLoader = Arc<dyn Fn(PageLoadRequest) + Send + Sync>;

#[derive(Clone)]
pub(super) struct PickerSourceConfig {
    pub(super) praxis_home: PathBuf,
    pub(super) page_loader: PageLoader,
}

#[derive(Clone)]
pub(super) struct PickerSourceEntry {
    pub(super) source: SessionLookupSource,
    pub(super) config: PickerSourceConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PickerSourceView {
    Source(SessionLookupSource),
    Archived,
}

impl PickerSourceView {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Source(source) => source.display_name(),
            Self::Archived => "Archived",
        }
    }

    pub(super) fn source(self) -> SessionLookupSource {
        match self {
            Self::Source(source) => source,
            Self::Archived => SessionLookupSource::Praxis,
        }
    }

    pub(super) fn archive_filter(self) -> ThreadArchiveFilter {
        match self {
            Self::Archived => ThreadArchiveFilter::Archived,
            Self::Source(_) => ThreadArchiveFilter::Active,
        }
    }

    pub(super) fn from_state(
        source: SessionLookupSource,
        archive_filter: ThreadArchiveFilter,
    ) -> Self {
        if source == SessionLookupSource::Praxis
            && matches!(archive_filter, ThreadArchiveFilter::Archived)
        {
            Self::Archived
        } else {
            Self::Source(source)
        }
    }
}

#[derive(Clone)]
pub(super) struct SourceSwitcher {
    pub(super) sources: Vec<PickerSourceEntry>,
}

impl SourceSwitcher {
    pub(super) fn from_sources(
        primary_source: SessionLookupSource,
        primary: PickerSourceConfig,
        alternate_source: SessionLookupSource,
        alternate: PickerSourceConfig,
    ) -> Self {
        Self {
            sources: vec![
                PickerSourceEntry {
                    source: primary_source,
                    config: primary,
                },
                PickerSourceEntry {
                    source: alternate_source,
                    config: alternate,
                },
            ],
        }
    }

    pub(super) fn config(&self, source: SessionLookupSource) -> Option<&PickerSourceConfig> {
        self.sources
            .iter()
            .find(|entry| entry.source == source)
            .map(|entry| &entry.config)
    }

    pub(super) fn views(&self) -> Vec<PickerSourceView> {
        let mut views = Vec::new();
        for source in [
            SessionLookupSource::Praxis,
            SessionLookupSource::Codex,
            SessionLookupSource::Cursor,
        ] {
            if self.config(source).is_none() {
                continue;
            }
            views.push(PickerSourceView::Source(source));
            if source == SessionLookupSource::Praxis {
                views.push(PickerSourceView::Archived);
            }
        }
        views
    }
}

pub(crate) struct AlternatePickerSource {
    pub(crate) source: SessionLookupSource,
    pub(crate) config: Config,
    pub(crate) app_gateway: AppGatewaySession,
}

pub(super) enum BackgroundEvent {
    PageLoaded {
        request_token: usize,
        search_token: Option<usize>,
        page: std::io::Result<PickerPage>,
    },
}

pub(super) type PageCursor = String;

pub(super) struct PickerPage {
    pub(super) rows: Vec<Row>,
    pub(super) next_cursor: Option<PageCursor>,
    pub(super) num_scanned_files: usize,
    pub(super) reached_scan_cap: bool,
}

pub(super) fn spawn_app_gateway_page_loader(
    app_gateway: AppGatewaySession,
    include_non_interactive: bool,
    bg_tx: mpsc::UnboundedSender<BackgroundEvent>,
) -> PageLoader {
    let (request_tx, mut request_rx) = mpsc::unbounded_channel::<PageLoadRequest>();

    tokio::spawn(async move {
        let mut app_gateway = app_gateway;
        while let Some(request) = request_rx.recv().await {
            let cursor = request.cursor;
            let page = load_app_gateway_page(
                &mut app_gateway,
                cursor,
                request.sort_key,
                include_non_interactive,
                request.search_term,
                request.filter_cwd,
                request.archive_filter,
            )
            .await;
            let _ = bg_tx.send(BackgroundEvent::PageLoaded {
                request_token: request.request_token,
                search_token: request.search_token,
                page,
            });
        }
        if let Err(err) = app_gateway.shutdown().await {
            warn!(%err, "Failed to shut down app-gateway picker session");
        }
    });

    Arc::new(move |request: PageLoadRequest| {
        let _ = request_tx.send(request);
    })
}

pub(super) fn picker_source_config(config: &Config, page_loader: PageLoader) -> PickerSourceConfig {
    PickerSourceConfig {
        praxis_home: config.praxis_home.clone(),
        page_loader,
    }
}
