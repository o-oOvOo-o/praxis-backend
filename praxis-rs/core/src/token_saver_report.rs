use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Datelike, FixedOffset, Local, TimeZone, Utc};
use praxis_protocol::protocol::{EventMsg, RolloutItem, RolloutLine, TokenSavingKind};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::thread_cost::{
    MODEL_PRICING_AS_OF, estimate_saved_input_cost_micros, input_price_usd_per_million_micros,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSaverReportPeriod {
    Month,
    Week,
    All,
}

#[derive(Debug, Clone)]
pub struct TokenSaverReportQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub utc_offset_minutes: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSaverReport {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub utc_offset_minutes: i32,
    pub total_saved_tokens: i64,
    pub priced_saved_tokens: i64,
    pub unpriced_saved_tokens: i64,
    pub estimated_saved_cost_micros: i64,
    pub scanned_threads: usize,
    pub contributing_threads: usize,
    pub samples: usize,
    pub pricing_basis: String,
    pub pricing_as_of: String,
    pub by_model: Vec<ModelSavingsRow>,
    pub by_category: Vec<SavingsRow>,
    pub by_day: Vec<SavingsRow>,
    pub by_thread: Vec<SavingsRow>,
    pub parse_errors: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSavingsRow {
    pub name: String,
    pub saved_tokens: i64,
    pub priced_saved_tokens: i64,
    pub unpriced_saved_tokens: i64,
    pub estimated_saved_cost_micros: i64,
    pub input_usd_per_million_micros: Option<i64>,
    pub samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavingsRow {
    pub name: String,
    pub saved_tokens: i64,
    pub estimated_saved_cost_micros: i64,
    pub samples: usize,
}

#[derive(Default)]
struct Totals {
    saved_tokens: i64,
    priced_saved_tokens: i64,
    unpriced_saved_tokens: i64,
    cost_micros: i64,
    samples: usize,
}

pub fn local_utc_offset_minutes() -> i32 {
    Local::now().offset().local_minus_utc() / 60
}

pub fn resolve_report_query(
    period: TokenSaverReportPeriod,
    from: Option<&str>,
    to: Option<&str>,
    utc_offset_minutes: i32,
) -> Result<TokenSaverReportQuery> {
    if !(-1439..=1439).contains(&utc_offset_minutes) {
        bail!("UTC offset must be between -1439 and 1439 minutes");
    }
    let offset = FixedOffset::east_opt(utc_offset_minutes * 60).context("invalid UTC offset")?;
    let now = Utc::now();
    let (from, to) = match (from, to) {
        (Some(from), Some(to)) => (parse_rfc3339(from, "from")?, parse_rfc3339(to, "to")?),
        (None, None) => {
            let local_now = now.with_timezone(&offset);
            let local_from = match period {
                TokenSaverReportPeriod::Month => offset
                    .with_ymd_and_hms(local_now.year(), local_now.month(), 1, 0, 0, 0)
                    .single()
                    .context("could not resolve month start")?,
                TokenSaverReportPeriod::Week => {
                    let date = local_now.date_naive()
                        - chrono::Duration::days(local_now.weekday().num_days_from_monday().into());
                    offset
                        .from_local_datetime(
                            &date.and_hms_opt(0, 0, 0).context("invalid week start")?,
                        )
                        .single()
                        .context("could not resolve week start")?
                }
                TokenSaverReportPeriod::All => offset
                    .timestamp_opt(0, 0)
                    .single()
                    .context("could not resolve epoch")?,
            };
            (local_from.with_timezone(&Utc), now)
        }
        _ => bail!("--from and --to must be provided together"),
    };
    if from >= to {
        bail!("report start must be earlier than report end");
    }
    Ok(TokenSaverReportQuery {
        from,
        to,
        utc_offset_minutes,
    })
}

fn parse_rfc3339(value: &str, name: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("invalid --{name} RFC3339 timestamp: {value}"))
        .map(|value| value.with_timezone(&Utc))
}

pub fn generate_token_saver_report(
    praxis_home: &Path,
    query: &TokenSaverReportQuery,
) -> Result<TokenSaverReport> {
    let offset =
        FixedOffset::east_opt(query.utc_offset_minutes * 60).context("invalid UTC offset")?;
    let mut paths = rollout_paths(praxis_home);
    paths.sort();

    let mut all = Totals::default();
    let mut models: BTreeMap<String, Totals> = BTreeMap::new();
    let mut categories: BTreeMap<String, Totals> = BTreeMap::new();
    let mut days: BTreeMap<String, Totals> = BTreeMap::new();
    let mut threads: BTreeMap<String, Totals> = BTreeMap::new();
    let mut parse_errors = 0usize;

    for path in &paths {
        scan_rollout(
            path,
            query,
            offset,
            &mut all,
            &mut models,
            &mut categories,
            &mut days,
            &mut threads,
            &mut parse_errors,
        )?;
    }

    let mut warnings = Vec::new();
    if all.unpriced_saved_tokens > 0 {
        warnings.push(format!(
            "{} saved tokens use models without a built-in input price and are excluded from the cost estimate.",
            all.unpriced_saved_tokens
        ));
    }
    if parse_errors > 0 {
        warnings.push(format!("Skipped {parse_errors} malformed rollout lines."));
    }

    Ok(TokenSaverReport {
        from: query.from,
        to: query.to,
        utc_offset_minutes: query.utc_offset_minutes,
        total_saved_tokens: all.saved_tokens,
        priced_saved_tokens: all.priced_saved_tokens,
        unpriced_saved_tokens: all.unpriced_saved_tokens,
        estimated_saved_cost_micros: all.cost_micros,
        scanned_threads: paths.len(),
        contributing_threads: threads.values().filter(|row| row.saved_tokens > 0).count(),
        samples: all.samples,
        pricing_basis: "API list input price estimate; not an invoice".to_string(),
        pricing_as_of: MODEL_PRICING_AS_OF.to_string(),
        by_model: sorted_model_rows(models),
        by_category: sorted_rows(categories, false),
        by_day: sorted_rows(days, true),
        by_thread: sorted_rows(threads, false),
        parse_errors,
        warnings,
    })
}

#[allow(clippy::too_many_arguments)]
fn scan_rollout(
    path: &Path,
    query: &TokenSaverReportQuery,
    offset: FixedOffset,
    all: &mut Totals,
    models: &mut BTreeMap<String, Totals>,
    categories: &mut BTreeMap<String, Totals>,
    days: &mut BTreeMap<String, Totals>,
    threads: &mut BTreeMap<String, Totals>,
    parse_errors: &mut usize,
) -> Result<()> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let fallback_thread = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mut thread = fallback_thread;
    let mut model = "unknown".to_string();
    let mut previous_total = 0i64;
    let mut previous_categories: BTreeMap<String, i64> = BTreeMap::new();

    for line in BufReader::new(file).lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => {
                *parse_errors += 1;
                continue;
            }
        };
        let rollout: RolloutLine = match serde_json::from_str(&line) {
            Ok(line) => line,
            Err(_) => {
                *parse_errors += 1;
                continue;
            }
        };
        match rollout.item {
            RolloutItem::SessionMeta(meta) => thread = meta.meta.id.to_string(),
            RolloutItem::TurnContext(context) => model = context.model,
            RolloutItem::EventMsg(EventMsg::TokenCount(event)) => {
                let Some(info) = event.info else { continue };
                let current = info.internal_savings.total_saved_tokens.max(0);
                let delta = cumulative_delta(previous_total, current);
                previous_total = current;

                let mut category_deltas = Vec::new();
                for category in info.internal_savings.categories {
                    let name = category_name(category.kind).to_string();
                    let current = category.total_saved_tokens.max(0);
                    let previous = previous_categories
                        .insert(name.clone(), current)
                        .unwrap_or(0);
                    let delta = cumulative_delta(previous, current);
                    if delta > 0 {
                        category_deltas.push((name, delta));
                    }
                }

                let timestamp = match DateTime::parse_from_rfc3339(&rollout.timestamp) {
                    Ok(value) => value.with_timezone(&Utc),
                    Err(_) => {
                        *parse_errors += 1;
                        continue;
                    }
                };
                if timestamp < query.from || timestamp >= query.to || delta <= 0 {
                    continue;
                }

                let cost = estimate_saved_input_cost_micros(&model, delta);
                add_total(all, delta, cost);
                add_total(models.entry(model.clone()).or_default(), delta, cost);
                add_total(threads.entry(thread.clone()).or_default(), delta, cost);
                let day = timestamp
                    .with_timezone(&offset)
                    .format("%Y-%m-%d")
                    .to_string();
                add_total(days.entry(day).or_default(), delta, cost);
                for (name, category_delta) in category_deltas {
                    let category_cost = estimate_saved_input_cost_micros(&model, category_delta);
                    add_total(
                        categories.entry(name).or_default(),
                        category_delta,
                        category_cost,
                    );
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn cumulative_delta(previous: i64, current: i64) -> i64 {
    if current >= previous {
        current - previous
    } else {
        current
    }
}

fn add_total(total: &mut Totals, tokens: i64, cost: Option<i64>) {
    total.saved_tokens = total.saved_tokens.saturating_add(tokens);
    total.samples = total.samples.saturating_add(1);
    if let Some(cost) = cost {
        total.priced_saved_tokens = total.priced_saved_tokens.saturating_add(tokens);
        total.cost_micros = total.cost_micros.saturating_add(cost);
    } else {
        total.unpriced_saved_tokens = total.unpriced_saved_tokens.saturating_add(tokens);
    }
}

fn rollout_paths(praxis_home: &Path) -> Vec<PathBuf> {
    ["sessions", "archived_sessions"]
        .into_iter()
        .flat_map(|dir| {
            WalkDir::new(praxis_home.join(dir))
                .follow_links(false)
                .into_iter()
        })
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "jsonl")
        })
        .map(|entry| entry.into_path())
        .collect()
}

fn category_name(kind: TokenSavingKind) -> &'static str {
    match kind {
        TokenSavingKind::OutputRepetition => "output_repetition",
        TokenSavingKind::OutputDelta => "output_delta",
        TokenSavingKind::ArtifactProjection => "artifact_projection",
        TokenSavingKind::UnchangedResource => "unchanged_resource",
        TokenSavingKind::SearchDelta => "search_delta",
        TokenSavingKind::ToolSchemaElision => "tool_schema_elision",
        TokenSavingKind::WorkingStateProjection => "working_state_projection",
        TokenSavingKind::ToolOutputProjection => "tool_output_projection",
    }
}

fn sorted_model_rows(map: BTreeMap<String, Totals>) -> Vec<ModelSavingsRow> {
    let mut rows: Vec<_> = map
        .into_iter()
        .map(|(name, row)| ModelSavingsRow {
            input_usd_per_million_micros: input_price_usd_per_million_micros(&name),
            name,
            saved_tokens: row.saved_tokens,
            priced_saved_tokens: row.priced_saved_tokens,
            unpriced_saved_tokens: row.unpriced_saved_tokens,
            estimated_saved_cost_micros: row.cost_micros,
            samples: row.samples,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.saved_tokens
            .cmp(&a.saved_tokens)
            .then_with(|| a.name.cmp(&b.name))
    });
    rows
}

fn sorted_rows(map: BTreeMap<String, Totals>, names_ascending: bool) -> Vec<SavingsRow> {
    let mut rows: Vec<_> = map
        .into_iter()
        .map(|(name, row)| SavingsRow {
            name,
            saved_tokens: row.saved_tokens,
            estimated_saved_cost_micros: row.cost_micros,
            samples: row.samples,
        })
        .collect();
    if !names_ascending {
        rows.sort_by(|a, b| {
            b.saved_tokens
                .cmp(&a.saved_tokens)
                .then_with(|| a.name.cmp(&b.name))
        });
    }
    rows
}

pub fn render_token_saver_report_markdown(report: &TokenSaverReport) -> String {
    let mut out = String::new();
    out.push_str("# Praxis Token Saver\n\n");
    out.push_str(&format!(
        "Period: `{}` to `{}` (UTC offset {:+03}:{:02})\n\n",
        report.from.to_rfc3339(),
        report.to.to_rfc3339(),
        report.utc_offset_minutes / 60,
        report.utc_offset_minutes.abs() % 60
    ));
    out.push_str("| Metric | Value |\n|---|---:|\n");
    out.push_str(&format!(
        "| Tokens saved | {} |\n",
        comma(report.total_saved_tokens)
    ));
    out.push_str(&format!(
        "| Estimated API input cost saved | {} |\n",
        dollars(report.estimated_saved_cost_micros)
    ));
    out.push_str(&format!(
        "| Priced / unpriced tokens | {} / {} |\n",
        comma(report.priced_saved_tokens),
        comma(report.unpriced_saved_tokens)
    ));
    out.push_str(&format!(
        "| Contributing / scanned threads | {} / {} |\n",
        report.contributing_threads, report.scanned_threads
    ));
    append_models(&mut out, &report.by_model);
    append_rows(&mut out, "By category", &report.by_category, None);
    append_rows(&mut out, "By day", &report.by_day, None);
    append_rows(&mut out, "Top threads", &report.by_thread, Some(10));
    out.push_str(&format!(
        "\n_Pricing basis: {}; prices verified {}._\n",
        report.pricing_basis, report.pricing_as_of
    ));
    for warning in &report.warnings {
        out.push_str(&format!("\n> {warning}\n"));
    }
    out
}

fn append_models(out: &mut String, rows: &[ModelSavingsRow]) {
    if rows.is_empty() {
        return;
    }
    out.push_str("\n## By model\n\n| Model | Tokens saved | Est. saved | Price / 1M input |\n|---|---:|---:|---:|\n");
    for row in rows {
        let price = row
            .input_usd_per_million_micros
            .map(dollars)
            .unwrap_or_else(|| "unpriced".to_string());
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            row.name,
            comma(row.saved_tokens),
            dollars(row.estimated_saved_cost_micros),
            price
        ));
    }
}

fn append_rows(out: &mut String, heading: &str, rows: &[SavingsRow], limit: Option<usize>) {
    if rows.is_empty() {
        return;
    }
    out.push_str(&format!(
        "\n## {heading}\n\n| Name | Tokens saved | Est. saved |\n|---|---:|---:|\n"
    ));
    for row in rows.iter().take(limit.unwrap_or(usize::MAX)) {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            row.name,
            comma(row.saved_tokens),
            dollars(row.estimated_saved_cost_micros)
        ));
    }
}

fn comma(value: i64) -> String {
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if value < 0 { format!("-{out}") } else { out }
}

fn dollars(micros: i64) -> String {
    format!("${:.4}", micros as f64 / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn range_uses_earlier_snapshot_as_baseline() {
        let home = tempfile::tempdir().expect("temp Praxis home");
        let sessions = home.path().join("sessions");
        fs::create_dir_all(&sessions).expect("sessions directory");
        let lines = [
            token_count_line("2026-08-01T00:00:00Z", 100, 100),
            token_count_line("2026-08-05T00:00:00Z", 160, 160),
            token_count_line("2026-09-01T00:00:00Z", 240, 240),
        ];
        fs::write(sessions.join("thread.jsonl"), lines.join("\n")).expect("rollout");
        let query = TokenSaverReportQuery {
            from: "2026-08-02T00:00:00Z".parse().expect("from"),
            to: "2026-09-01T00:00:00Z".parse().expect("to"),
            utc_offset_minutes: 0,
        };

        let report = generate_token_saver_report(home.path(), &query).expect("report");

        assert_eq!(report.total_saved_tokens, 60);
        assert_eq!(report.unpriced_saved_tokens, 60);
        assert_eq!(report.by_category[0].saved_tokens, 60);
        assert_eq!(report.by_day[0].name, "2026-08-05");
        assert!(render_token_saver_report_markdown(&report).contains("not an invoice"));
    }

    fn token_count_line(timestamp: &str, total: i64, category_total: i64) -> String {
        json!({
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {},
                    "last_token_usage": {},
                    "internal_savings": {
                        "total_saved_tokens": total,
                        "last_saved_tokens": total,
                        "categories": [{
                            "kind": "tool_output_projection",
                            "total_saved_tokens": category_total,
                            "last_saved_tokens": category_total,
                            "occurrences": 1
                        }]
                    },
                    "model_context_window": null
                },
                "rate_limits": null
            }
        })
        .to_string()
    }
}
