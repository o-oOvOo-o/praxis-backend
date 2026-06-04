use std::collections::BTreeMap;

use praxis_app_gateway_protocol::ThreadListResponse;
use praxis_protocol::protocol::TokenUsage;
use ratatui::prelude::*;
use ratatui::style::Stylize;

use crate::app_gateway_session::token_usage_info_from_app_gateway;
use crate::history_cell::PlainHistoryCell;
use crate::status::format_tokens_compact;

pub(crate) const DEFAULT_TOKEN_USAGE_THREAD_LIMIT: usize = 20;
pub(crate) const MAX_TOKEN_USAGE_THREAD_LIMIT: usize = 100;

#[derive(Clone, Debug, Default)]
struct TokenUsageAggregate {
    thread_count: usize,
    total: TokenUsage,
    last: TokenUsage,
}

impl TokenUsageAggregate {
    fn add_thread(&mut self, total: &TokenUsage, last: &TokenUsage) {
        self.thread_count += 1;
        self.total.add_assign(total);
        self.last.add_assign(last);
    }
}

pub(crate) fn parse_token_usage_limit(raw: &str) -> Result<usize, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_TOKEN_USAGE_THREAD_LIMIT);
    }

    let mut parts = trimmed.split_whitespace();
    let Some(first) = parts.next() else {
        return Ok(DEFAULT_TOKEN_USAGE_THREAD_LIMIT);
    };
    if parts.next().is_some() || !first.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("Usage: /token [1-100], /token30, or /token 30".to_string());
    }

    let limit = first
        .parse::<usize>()
        .map_err(|_| "Usage: /token [1-100], /token30, or /token 30".to_string())?;
    if limit == 0 {
        return Err("Usage: /token [1-100], /token30, or /token 30".to_string());
    }
    Ok(limit.min(MAX_TOKEN_USAGE_THREAD_LIMIT))
}

pub(crate) fn token_usage_summary_cell(
    requested_limit: usize,
    response: ThreadListResponse,
) -> PlainHistoryCell {
    let next_cursor = response.next_cursor;
    let scanned_threads = response.data.len();
    let mut total = TokenUsageAggregate::default();
    let mut by_model: BTreeMap<(String, String), TokenUsageAggregate> = BTreeMap::new();

    for thread in response.data {
        let Some(raw_usage) = thread.token_usage else {
            continue;
        };
        let usage = token_usage_info_from_app_gateway(raw_usage);
        let model_provider = thread.model_provider;
        let model = thread.model.unwrap_or_else(|| "*".to_string());
        total.add_thread(&usage.total_token_usage, &usage.last_token_usage);
        by_model
            .entry((model_provider, model))
            .or_default()
            .add_thread(&usage.total_token_usage, &usage.last_token_usage);
    }

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        "• ".dim(),
        "Praxis token usage".bold(),
        format!(
            "  {} threads with usage / {} recent scanned",
            total.thread_count, scanned_threads
        )
        .dim(),
    ]));

    if total.thread_count == 0 {
        lines.push(Line::from(
            "  No token usage has been recorded for these threads.".dark_gray(),
        ));
        return PlainHistoryCell::new(lines);
    }

    lines.push(Line::from(vec![
        "  total ".dark_gray(),
        usage_line(&total.total).into(),
        "  ".into(),
        cache_line(&total.total).dark_gray(),
    ]));
    lines.push(Line::from(vec![
        "  last  ".dark_gray(),
        usage_line(&total.last).into(),
    ]));

    let mut rows = by_model.into_iter().collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.1.total
            .total_tokens
            .cmp(&a.1.total.total_tokens)
            .then_with(|| a.0.cmp(&b.0))
    });

    lines.push(Line::from(""));
    lines.push(Line::from("  provider/model".dark_gray()));
    for ((provider, model), aggregate) in rows {
        lines.push(Line::from(vec![
            "  ".into(),
            format!("{provider}/{model}").green(),
            format!("  {} threads  ", aggregate.thread_count).dark_gray(),
            usage_line(&aggregate.total).into(),
            "  ".into(),
            cache_line(&aggregate.total).dark_gray(),
        ]));
    }

    if next_cursor.is_some() && requested_limit < MAX_TOKEN_USAGE_THREAD_LIMIT {
        lines.push(Line::from(""));
        lines.push(Line::from(
            format!(
                "  More threads exist. Use /token{} for a wider recent-window scan.",
                MAX_TOKEN_USAGE_THREAD_LIMIT
            )
            .dark_gray(),
        ));
    }

    PlainHistoryCell::new(lines)
}

fn usage_line(usage: &TokenUsage) -> String {
    format!(
        "{} total | {} in | {} cached | {} out | {} reasoning",
        format_tokens_compact(usage.total_tokens),
        format_tokens_compact(usage.input_tokens),
        format_tokens_compact(usage.cached_input()),
        format_tokens_compact(usage.output_tokens),
        format_tokens_compact(usage.reasoning_output_tokens),
    )
}

fn cache_line(usage: &TokenUsage) -> String {
    match usage.cache_hit_percent() {
        Some(percent) => format!(
            "cache {percent}% ({}/{})",
            format_tokens_compact(usage.cached_input().min(usage.cache_reported_input())),
            format_tokens_compact(usage.cache_reported_input())
        ),
        None => "cache n/a".to_string(),
    }
}
