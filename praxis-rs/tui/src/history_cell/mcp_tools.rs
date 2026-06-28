use super::*;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;

#[derive(Debug)]
struct CompletedMcpToolCallWithImageOutput {
    _image: DynamicImage,
}
impl HistoryCell for CompletedMcpToolCallWithImageOutput {
    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        vec!["tool result (image output)".into()]
    }
}

#[derive(Debug)]
pub(crate) struct McpToolCallCell {
    call_id: String,
    invocation: McpInvocation,
    start_time: Instant,
    duration: Option<Duration>,
    result: Option<Result<praxis_protocol::mcp::CallToolResult, String>>,
    animations_enabled: bool,
}

impl McpToolCallCell {
    pub(crate) fn new(
        call_id: String,
        invocation: McpInvocation,
        animations_enabled: bool,
    ) -> Self {
        Self {
            call_id,
            invocation,
            start_time: Instant::now(),
            duration: None,
            result: None,
            animations_enabled,
        }
    }

    pub(crate) fn call_id(&self) -> &str {
        &self.call_id
    }

    pub(crate) fn complete(
        &mut self,
        duration: Duration,
        result: Result<praxis_protocol::mcp::CallToolResult, String>,
    ) -> Option<Box<dyn HistoryCell>> {
        let image_cell = try_new_completed_mcp_tool_call_with_image_output(&result)
            .map(|cell| Box::new(cell) as Box<dyn HistoryCell>);
        self.duration = Some(duration);
        self.result = Some(result);
        image_cell
    }

    fn success(&self) -> Option<bool> {
        match self.result.as_ref() {
            Some(Ok(result)) => Some(!result.is_error.unwrap_or(false)),
            Some(Err(_)) => Some(false),
            None => None,
        }
    }

    fn card_id(&self) -> TranscriptCardId {
        TranscriptCardId::mcp(self.call_id.clone())
    }

    fn is_card_expanded(&self) -> bool {
        is_transcript_card_expanded(&self.card_id())
    }

    pub(crate) fn mark_failed(&mut self) {
        let elapsed = self.start_time.elapsed();
        self.duration = Some(elapsed);
        self.result = Some(Err("interrupted".to_string()));
    }

    fn render_content_block(
        block: &serde_json::Value,
        width: usize,
        max_lines: Option<usize>,
    ) -> String {
        let format_tool_text = |text: &str| match max_lines {
            Some(max_lines) => format_and_truncate_tool_result(text, max_lines, width),
            None => format_json_compact(text).unwrap_or_else(|| text.to_string()),
        };

        let content = match serde_json::from_value::<rmcp::model::Content>(block.clone()) {
            Ok(content) => content,
            Err(_) => {
                return format_tool_text(&block.to_string());
            }
        };

        match content.raw {
            rmcp::model::RawContent::Text(text) => format_tool_text(&text.text),
            rmcp::model::RawContent::Image(_) => "<image content>".to_string(),
            rmcp::model::RawContent::Audio(_) => "<audio content>".to_string(),
            rmcp::model::RawContent::Resource(resource) => {
                let uri = match resource.resource {
                    rmcp::model::ResourceContents::TextResourceContents { uri, .. } => uri,
                    rmcp::model::ResourceContents::BlobResourceContents { uri, .. } => uri,
                };
                format!("embedded resource: {uri}")
            }
            rmcp::model::RawContent::ResourceLink(link) => format!("link: {}", link.uri),
        }
    }

    fn detail_lines(&self, width: u16, expanded: bool) -> Vec<Line<'static>> {
        let mut detail_lines: Vec<Line<'static>> = Vec::new();
        let detail_wrap_width = (width as usize).saturating_sub(4).max(1);
        let max_lines = (!expanded).then_some(TOOL_CALL_MAX_LINES);

        if let Some(result) = &self.result {
            match result {
                Ok(praxis_protocol::mcp::CallToolResult { content, .. }) => {
                    if !content.is_empty() {
                        for block in content {
                            let text =
                                Self::render_content_block(block, detail_wrap_width, max_lines);
                            for segment in text.split('\n') {
                                let line = Line::from(segment.to_string().dim());
                                let wrapped = adaptive_wrap_line(
                                    &line,
                                    RtOptions::new(detail_wrap_width)
                                        .initial_indent("".into())
                                        .subsequent_indent("    ".into()),
                                );
                                detail_lines.extend(wrapped.iter().map(line_to_static));
                            }
                        }
                    }
                }
                Err(err) => {
                    let err_text = match max_lines {
                        Some(max_lines) => format_and_truncate_tool_result(
                            &format!("Error: {err}"),
                            max_lines,
                            width as usize,
                        ),
                        None => format!("Error: {err}"),
                    };
                    let err_line = Line::from(err_text.dim());
                    let wrapped = adaptive_wrap_line(
                        &err_line,
                        RtOptions::new(detail_wrap_width)
                            .initial_indent("".into())
                            .subsequent_indent("    ".into()),
                    );
                    detail_lines.extend(wrapped.iter().map(line_to_static));
                }
            }
        }

        detail_lines
    }
}

impl HistoryCell for McpToolCallCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        let status = self.success();
        let expanded = self.is_card_expanded();
        let bullet = match status {
            Some(true) => "•".green().bold(),
            Some(false) => "•".red().bold(),
            None => spinner(Some(self.start_time), self.animations_enabled),
        };
        let header_text = if status.is_some() {
            "Called"
        } else {
            "Calling"
        };

        let invocation_line = line_to_static(&format_mcp_invocation(self.invocation.clone()));
        let marker = if expanded { "▾ " } else { "▸ " };
        let mut compact_spans = vec![
            marker.dim(),
            bullet.clone(),
            " ".into(),
            header_text.bold(),
            " ".into(),
        ];
        let mut compact_header = Line::from(compact_spans.clone());
        let reserved = compact_header.width();

        if !expanded {
            compact_header.extend(invocation_line.spans.clone());
            return vec![truncate_line_with_ellipsis_if_overflow(
                compact_header,
                usize::from(width.max(1)),
            )];
        }

        let inline_invocation =
            invocation_line.width() <= (width as usize).saturating_sub(reserved);

        if inline_invocation {
            compact_header.extend(invocation_line.spans.clone());
            lines.push(compact_header);
        } else {
            compact_spans.pop(); // drop trailing space for standalone header
            lines.push(Line::from(compact_spans));

            let opts = RtOptions::new((width as usize).saturating_sub(4))
                .initial_indent("".into())
                .subsequent_indent("    ".into());
            let wrapped = adaptive_wrap_line(&invocation_line, opts);
            let body_lines: Vec<Line<'static>> = wrapped.iter().map(line_to_static).collect();
            lines.extend(prefix_lines(body_lines, "  └ ".dim(), "    ".into()));
        }

        let detail_lines = self.detail_lines(width, expanded);

        if expanded && !detail_lines.is_empty() {
            let initial_prefix: Span<'static> = if inline_invocation {
                "  └ ".dim()
            } else {
                "    ".into()
            };
            lines.extend(prefix_lines(detail_lines, initial_prefix, "    ".into()));
        }

        lines
    }

    fn mouse_targets(&self, _width: u16) -> Vec<HistoryCellMouseTarget> {
        vec![HistoryCellMouseTarget {
            row_start: 0,
            row_end: 0,
            action: HistoryCellMouseAction::ToggleTranscriptCard {
                card_id: self.card_id(),
            },
        }]
    }

    fn transcript_animation_tick(&self) -> Option<u64> {
        if !self.animations_enabled || self.result.is_some() {
            return None;
        }
        Some((self.start_time.elapsed().as_millis() / 50) as u64)
    }
}

pub(crate) fn new_active_mcp_tool_call(
    call_id: String,
    invocation: McpInvocation,
    animations_enabled: bool,
) -> McpToolCallCell {
    McpToolCallCell::new(call_id, invocation, animations_enabled)
}

fn web_search_header(completed: bool) -> &'static str {
    if completed {
        "Searched"
    } else {
        "Searching the web"
    }
}

#[derive(Debug)]
pub(crate) struct WebSearchCell {
    call_id: String,
    query: String,
    action: Option<WebSearchAction>,
    start_time: Instant,
    completed: bool,
    animations_enabled: bool,
}

impl WebSearchCell {
    pub(crate) fn new(
        call_id: String,
        query: String,
        action: Option<WebSearchAction>,
        animations_enabled: bool,
    ) -> Self {
        Self {
            call_id,
            query,
            action,
            start_time: Instant::now(),
            completed: false,
            animations_enabled,
        }
    }

    pub(crate) fn call_id(&self) -> &str {
        &self.call_id
    }

    pub(crate) fn update(&mut self, action: WebSearchAction, query: String) {
        self.action = Some(action);
        self.query = query;
    }

    pub(crate) fn complete(&mut self) {
        self.completed = true;
    }

    fn card_id(&self) -> TranscriptCardId {
        TranscriptCardId::web_search(self.call_id.clone())
    }

    fn is_card_expanded(&self) -> bool {
        is_transcript_card_expanded(&self.card_id())
    }
}

impl HistoryCell for WebSearchCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let expanded = self.is_card_expanded();
        let bullet = if self.completed {
            "•".dim()
        } else {
            spinner(Some(self.start_time), self.animations_enabled)
        };
        let marker = if expanded { "▾ " } else { "▸ " };
        let header = web_search_header(self.completed);
        if !expanded {
            let line = Line::from(vec![
                marker.dim(),
                bullet,
                " ".into(),
                header.bold(),
                " ".into(),
                self.query.clone().into(),
            ]);
            return vec![truncate_line_with_ellipsis_if_overflow(
                line,
                usize::from(width.max(1)),
            )];
        }
        let detail = web_search_detail(self.action.as_ref(), &self.query);
        let text: Text<'static> = if detail.is_empty() {
            Line::from(vec![header.bold()]).into()
        } else {
            Line::from(vec![header.bold(), " ".into(), detail.into()]).into()
        };
        PrefixedWrappedHistoryCell::new(text, vec![marker.dim(), bullet, " ".into()], "  ")
            .display_lines(width)
    }

    fn mouse_targets(&self, _width: u16) -> Vec<HistoryCellMouseTarget> {
        vec![HistoryCellMouseTarget {
            row_start: 0,
            row_end: 0,
            action: HistoryCellMouseAction::ToggleTranscriptCard {
                card_id: self.card_id(),
            },
        }]
    }
}

pub(crate) fn new_active_web_search_call(
    call_id: String,
    query: String,
    animations_enabled: bool,
) -> WebSearchCell {
    WebSearchCell::new(call_id, query, /*action*/ None, animations_enabled)
}

pub(crate) fn new_web_search_call(
    call_id: String,
    query: String,
    action: WebSearchAction,
) -> WebSearchCell {
    let mut cell = WebSearchCell::new(
        call_id,
        query,
        Some(action),
        /*animations_enabled*/ false,
    );
    cell.complete();
    cell
}

/// Returns an additional history cell if an MCP tool result includes a decodable image.
///
/// This intentionally returns at most one cell: the first image in `CallToolResult.content` that
/// successfully base64-decodes and parses as an image. This is used as a lightweight “image output
/// exists” affordance separate from the main MCP tool call cell.
///
/// Manual testing tip:
/// - Run the rmcp stdio test server (`praxis-rs/rmcp-client/src/bin/test_stdio_server.rs`) and
///   register it as an MCP server via `praxis mcp add`.
/// - Use its `image_scenario` tool with cases like `text_then_image`,
///   `invalid_base64_then_image`, or `invalid_image_bytes_then_image` to ensure this path triggers
///   even when the first block is not a valid image.
fn try_new_completed_mcp_tool_call_with_image_output(
    result: &Result<praxis_protocol::mcp::CallToolResult, String>,
) -> Option<CompletedMcpToolCallWithImageOutput> {
    let image = result
        .as_ref()
        .ok()?
        .content
        .iter()
        .find_map(decode_mcp_image)?;

    Some(CompletedMcpToolCallWithImageOutput { _image: image })
}

/// Decodes an MCP `ImageContent` block into an in-memory image.
///
/// Returns `None` when the block is not an image, when base64 decoding fails, when the format
/// cannot be inferred, or when the image decoder rejects the bytes.
fn decode_mcp_image(block: &serde_json::Value) -> Option<DynamicImage> {
    let content = serde_json::from_value::<rmcp::model::Content>(block.clone()).ok()?;
    let rmcp::model::RawContent::Image(image) = content.raw else {
        return None;
    };
    let base64_data = if let Some(data_url) = image.data.strip_prefix("data:") {
        data_url.split_once(',')?.1
    } else {
        image.data.as_str()
    };
    let raw_data = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| {
            error!("Failed to decode image data: {e}");
            e
        })
        .ok()?;
    let reader = ImageReader::new(Cursor::new(raw_data))
        .with_guessed_format()
        .map_err(|e| {
            error!("Failed to guess image format: {e}");
            e
        })
        .ok()?;

    reader
        .decode()
        .map_err(|e| {
            error!("Image decoding failed: {e}");
            e
        })
        .ok()
}

#[allow(clippy::disallowed_methods)]
pub(crate) fn new_warning_event(message: String) -> PrefixedWrappedHistoryCell {
    PrefixedWrappedHistoryCell::new(message.yellow(), "⚠ ".yellow(), "  ")
}

#[derive(Debug)]
pub(crate) struct DeprecationNoticeCell {
    summary: String,
    details: Option<String>,
}

pub(crate) fn new_deprecation_notice(
    summary: String,
    details: Option<String>,
) -> DeprecationNoticeCell {
    DeprecationNoticeCell { summary, details }
}

impl HistoryCell for DeprecationNoticeCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(vec!["⚠ ".red().bold(), self.summary.clone().red()].into());

        let wrap_width = width.saturating_sub(4).max(1) as usize;

        if let Some(details) = &self.details {
            let detail_line = Line::from(details.clone().dim());
            let wrapped = adaptive_wrap_line(&detail_line, RtOptions::new(wrap_width));
            push_owned_lines(&wrapped, &mut lines);
        }

        lines
    }
}

/// Render a summary of configured MCP servers from the current `Config`.
pub(crate) fn empty_mcp_output() -> PlainHistoryCell {
    let lines: Vec<Line<'static>> = vec![
        "/mcp".magenta().into(),
        "".into(),
        vec!["🔌  ".into(), "MCP Tools".bold()].into(),
        "".into(),
        "  • No MCP servers configured.".italic().into(),
        Line::from(vec![
            "    See the ".into(),
            "\u{1b}]8;;https://github.com/o-oOvOo-o/praxis-backend/blob/main/docs/config.md#connecting-to-mcp-servers\u{7}MCP docs\u{1b}]8;;\u{7}"
                .underlined(),
            " to configure them.".into(),
        ])
        .style(Style::default().add_modifier(Modifier::DIM)),
    ];

    PlainHistoryCell { lines }
}

#[cfg(test)]
/// Render MCP tools grouped by connection using the fully-qualified tool names.
pub(crate) fn new_mcp_tools_output(
    config: &Config,
    tools: HashMap<String, praxis_protocol::mcp::Tool>,
    resources: HashMap<String, Vec<Resource>>,
    resource_templates: HashMap<String, Vec<ResourceTemplate>>,
    auth_statuses: &HashMap<String, McpAuthStatus>,
) -> PlainHistoryCell {
    let mut lines: Vec<Line<'static>> = vec![
        "/mcp".magenta().into(),
        "".into(),
        vec!["🔌  ".into(), "MCP Tools".bold()].into(),
        "".into(),
    ];

    if tools.is_empty() {
        lines.push("  • No MCP tools available.".italic().into());
        lines.push("".into());
    }

    let mcp_manager = McpManager::new(Arc::new(PluginsManager::new(config.praxis_home.clone())));
    let effective_servers = mcp_manager.effective_servers(config, /*auth*/ None);
    let mut servers: Vec<_> = effective_servers.iter().collect();
    servers.sort_by(|(a, _), (b, _)| a.cmp(b));

    for (server, cfg) in servers {
        let prefix = qualified_mcp_tool_name_prefix(server);
        let mut names: Vec<String> = tools
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .map(|k| k[prefix.len()..].to_string())
            .collect();
        names.sort();

        let auth_status = auth_statuses
            .get(server.as_str())
            .copied()
            .unwrap_or(McpAuthStatus::Unsupported);
        let mut header: Vec<Span<'static>> = vec!["  • ".into(), server.clone().into()];
        if !cfg.enabled {
            header.push(" ".into());
            header.push("(disabled)".red());
            lines.push(header.into());
            if let Some(reason) = cfg.disabled_reason.as_ref().map(ToString::to_string) {
                lines.push(vec!["    • Reason: ".into(), reason.dim()].into());
            }
            lines.push(Line::from(""));
            continue;
        }
        lines.push(header.into());
        lines.push(vec!["    • Status: ".into(), "enabled".green()].into());
        lines.push(vec!["    • Auth: ".into(), auth_status.to_string().into()].into());

        match &cfg.transport {
            McpServerTransportConfig::Stdio {
                command,
                args,
                env,
                env_vars,
                cwd,
            } => {
                let args_suffix = if args.is_empty() {
                    String::new()
                } else {
                    format!(" {}", args.join(" "))
                };
                let cmd_display = format!("{command}{args_suffix}");
                lines.push(vec!["    • Command: ".into(), cmd_display.into()].into());

                if let Some(cwd) = cwd.as_ref() {
                    lines.push(vec!["    • Cwd: ".into(), cwd.display().to_string().into()].into());
                }

                let env_display = format_env_display(env.as_ref(), env_vars);
                if env_display != "-" {
                    lines.push(vec!["    • Env: ".into(), env_display.into()].into());
                }
            }
            McpServerTransportConfig::StreamableHttp {
                url,
                http_headers,
                env_http_headers,
                ..
            } => {
                lines.push(vec!["    • URL: ".into(), url.clone().into()].into());
                if let Some(headers) = http_headers.as_ref()
                    && !headers.is_empty()
                {
                    let mut pairs: Vec<_> = headers.iter().collect();
                    pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                    let display = pairs
                        .into_iter()
                        .map(|(name, _)| format!("{name}=*****"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(vec!["    • HTTP headers: ".into(), display.into()].into());
                }
                if let Some(headers) = env_http_headers.as_ref()
                    && !headers.is_empty()
                {
                    let mut pairs: Vec<_> = headers.iter().collect();
                    pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                    let display = pairs
                        .into_iter()
                        .map(|(name, var)| format!("{name}={var}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(vec!["    • Env HTTP headers: ".into(), display.into()].into());
                }
            }
        }

        if names.is_empty() {
            lines.push("    • Tools: (none)".into());
        } else {
            lines.push(vec!["    • Tools: ".into(), names.join(", ").into()].into());
        }

        let server_resources: Vec<Resource> =
            resources.get(server.as_str()).cloned().unwrap_or_default();
        if server_resources.is_empty() {
            lines.push("    • Resources: (none)".into());
        } else {
            let mut spans: Vec<Span<'static>> = vec!["    • Resources: ".into()];

            for (idx, resource) in server_resources.iter().enumerate() {
                if idx > 0 {
                    spans.push(", ".into());
                }

                let label = resource.title.as_ref().unwrap_or(&resource.name);
                spans.push(label.clone().into());
                spans.push(" ".into());
                spans.push(format!("({})", resource.uri).dim());
            }

            lines.push(spans.into());
        }

        let server_templates: Vec<ResourceTemplate> = resource_templates
            .get(server.as_str())
            .cloned()
            .unwrap_or_default();
        if server_templates.is_empty() {
            lines.push("    • Resource templates: (none)".into());
        } else {
            let mut spans: Vec<Span<'static>> = vec!["    • Resource templates: ".into()];

            for (idx, template) in server_templates.iter().enumerate() {
                if idx > 0 {
                    spans.push(", ".into());
                }

                let label = template.title.as_ref().unwrap_or(&template.name);
                spans.push(label.clone().into());
                spans.push(" ".into());
                spans.push(format!("({})", template.uri_template).dim());
            }

            lines.push(spans.into());
        }

        lines.push(Line::from(""));
    }

    PlainHistoryCell { lines }
}

/// Build the `/mcp` history cell from app-gateway `McpServerStatus` responses.
///
/// The server list comes directly from the app-gateway status response, sorted
/// alphabetically. Local config is only used to enrich returned servers with
/// transport details such as command, URL, cwd, and environment display.
///
/// This mirrors the layout of [`new_mcp_tools_output`] but sources data from
/// the paginated RPC response rather than the in-process `McpManager`.
pub(crate) fn new_mcp_tools_output_from_statuses(
    config: &Config,
    statuses: &[McpServerStatus],
) -> PlainHistoryCell {
    let mut lines: Vec<Line<'static>> = vec![
        "/mcp".magenta().into(),
        "".into(),
        vec!["🔌  ".into(), "MCP Tools".bold()].into(),
        "".into(),
    ];

    let mut statuses_by_name = HashMap::new();
    for status in statuses {
        statuses_by_name.insert(status.name.as_str(), status);
    }

    let mut server_names: Vec<String> = statuses.iter().map(|status| status.name.clone()).collect();
    server_names.sort();

    let has_any_tools = statuses.iter().any(|status| !status.tools.is_empty());
    if !has_any_tools {
        lines.push("  • No MCP tools available.".italic().into());
        lines.push("".into());
    }

    for server in server_names {
        let cfg = config.mcp_servers.get().get(server.as_str());
        let status = statuses_by_name.get(server.as_str()).copied();
        let header: Vec<Span<'static>> = vec!["  • ".into(), server.clone().into()];

        lines.push(header.into());
        let auth_status = status
            .map(|status| match status.auth_status {
                praxis_app_gateway_protocol::McpAuthStatus::Unsupported => {
                    McpAuthStatus::Unsupported
                }
                praxis_app_gateway_protocol::McpAuthStatus::NotLoggedIn => {
                    McpAuthStatus::NotLoggedIn
                }
                praxis_app_gateway_protocol::McpAuthStatus::BearerToken => {
                    McpAuthStatus::BearerToken
                }
                praxis_app_gateway_protocol::McpAuthStatus::OAuth => McpAuthStatus::OAuth,
            })
            .unwrap_or(McpAuthStatus::Unsupported);
        lines.push(vec!["    • Auth: ".into(), auth_status.to_string().into()].into());

        if let Some(cfg) = cfg {
            match &cfg.transport {
                McpServerTransportConfig::Stdio {
                    command,
                    args,
                    env,
                    env_vars,
                    cwd,
                } => {
                    let args_suffix = if args.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", args.join(" "))
                    };
                    let cmd_display = format!("{command}{args_suffix}");
                    lines.push(vec!["    • Command: ".into(), cmd_display.into()].into());

                    if let Some(cwd) = cwd.as_ref() {
                        lines.push(
                            vec!["    • Cwd: ".into(), cwd.display().to_string().into()].into(),
                        );
                    }

                    let env_display = format_env_display(env.as_ref(), env_vars.as_slice());
                    if env_display != "-" {
                        lines.push(vec!["    • Env: ".into(), env_display.into()].into());
                    }
                }
                McpServerTransportConfig::StreamableHttp {
                    url,
                    http_headers,
                    env_http_headers,
                    ..
                } => {
                    lines.push(vec!["    • URL: ".into(), url.clone().into()].into());
                    if let Some(headers) = http_headers.as_ref()
                        && !headers.is_empty()
                    {
                        let mut pairs: Vec<_> = headers.iter().collect();
                        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                        let display = pairs
                            .into_iter()
                            .map(|(name, _)| format!("{name}=*****"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        lines.push(vec!["    • HTTP headers: ".into(), display.into()].into());
                    }
                    if let Some(headers) = env_http_headers.as_ref()
                        && !headers.is_empty()
                    {
                        let mut pairs: Vec<_> = headers.iter().collect();
                        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                        let display = pairs
                            .into_iter()
                            .map(|(name, var)| format!("{name}={var}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        lines.push(vec!["    • Env HTTP headers: ".into(), display.into()].into());
                    }
                }
            }
        }

        let mut names = status
            .map(|status| status.tools.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        names.sort();
        if names.is_empty() {
            lines.push("    • Tools: (none)".into());
        } else {
            lines.push(vec!["    • Tools: ".into(), names.join(", ").into()].into());
        }

        let server_resources = status
            .map(|status| status.resources.clone())
            .unwrap_or_default();
        if server_resources.is_empty() {
            lines.push("    • Resources: (none)".into());
        } else {
            let mut spans: Vec<Span<'static>> = vec!["    • Resources: ".into()];

            for (idx, resource) in server_resources.iter().enumerate() {
                if idx > 0 {
                    spans.push(", ".into());
                }

                let label = resource.title.as_ref().unwrap_or(&resource.name);
                spans.push(label.clone().into());
                spans.push(" ".into());
                spans.push(format!("({})", resource.uri).dim());
            }

            lines.push(spans.into());
        }

        let server_templates = status
            .map(|status| status.resource_templates.clone())
            .unwrap_or_default();
        if server_templates.is_empty() {
            lines.push("    • Resource templates: (none)".into());
        } else {
            let mut spans: Vec<Span<'static>> = vec!["    • Resource templates: ".into()];

            for (idx, template) in server_templates.iter().enumerate() {
                if idx > 0 {
                    spans.push(", ".into());
                }

                let label = template.title.as_ref().unwrap_or(&template.name);
                spans.push(label.clone().into());
                spans.push(" ".into());
                spans.push(format!("({})", template.uri_template).dim());
            }

            lines.push(spans.into());
        }

        lines.push(Line::from(""));
    }

    PlainHistoryCell { lines }
}
