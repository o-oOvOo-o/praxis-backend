use chrono::DateTime;
use chrono::Duration as ChronoDuration;
use chrono::Utc;
use cron::Schedule;
use praxis_state::AutomationKind;
use serde_json::Value as JsonValue;
use std::str::FromStr;

pub(crate) enum AutomationSchedule {
    Heartbeat { interval_ms: i64 },
    Cron { schedule: Schedule },
}

impl AutomationSchedule {
    pub(crate) fn parse(kind: AutomationKind, value: &JsonValue) -> Result<Self, String> {
        validate_schedule_object(value)?;
        match kind {
            AutomationKind::Heartbeat => {
                let interval_ms = value
                    .get("intervalMs")
                    .and_then(JsonValue::as_i64)
                    .ok_or_else(|| "heartbeat schedule requires intervalMs".to_string())?;
                if interval_ms <= 0 {
                    return Err(
                        "heartbeat schedule intervalMs must be greater than zero".to_string()
                    );
                }
                Ok(Self::Heartbeat { interval_ms })
            }
            AutomationKind::Cron => {
                let cron = value
                    .get("cron")
                    .and_then(JsonValue::as_str)
                    .map(str::trim)
                    .unwrap_or_default();
                if cron.is_empty() {
                    return Err("cron schedule requires cron".to_string());
                }
                let cron = normalize_cron_expression(cron)?;
                let schedule = Schedule::from_str(cron.as_str())
                    .map_err(|err| format!("invalid cron expression: {err}"))?;
                Ok(Self::Cron { schedule })
            }
        }
    }

    pub(crate) fn next_run_at(&self, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, String> {
        match self {
            Self::Heartbeat { interval_ms } => {
                Ok(Some(now + ChronoDuration::milliseconds(*interval_ms)))
            }
            Self::Cron { schedule } => schedule
                .upcoming(Utc)
                .next()
                .ok_or_else(|| "cron schedule produced no future run".to_string())
                .map(Some),
        }
    }
}

pub(crate) fn validate_automation_schedule(
    kind: AutomationKind,
    value: &JsonValue,
) -> Result<(), String> {
    AutomationSchedule::parse(kind, value).map(|_| ())
}

pub(crate) fn next_automation_run_at(
    kind: AutomationKind,
    value: &JsonValue,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, String> {
    AutomationSchedule::parse(kind, value)?.next_run_at(now)
}

fn validate_schedule_object(value: &JsonValue) -> Result<(), String> {
    if value.is_object() {
        Ok(())
    } else {
        Err("schedule must be a JSON object".to_string())
    }
}

fn normalize_cron_expression(cron: &str) -> Result<String, String> {
    let fields: Vec<&str> = cron.split_whitespace().collect();
    match fields.len() {
        5 => Ok(format!("0 {cron}")),
        6 | 7 => Ok(cron.to_string()),
        _ => Err("cron schedule must use 5, 6, or 7 fields".to_string()),
    }
}
