use praxis_tui::SessionLookupSource;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionTargetSource {
    Praxis,
    Codex,
    Cursor,
}

impl SessionTargetSource {
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "praxis" => Some(Self::Praxis),
            "codex" => Some(Self::Codex),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }

    pub(crate) fn lookup_source(self) -> SessionLookupSource {
        match self {
            Self::Praxis => SessionLookupSource::Praxis,
            Self::Codex => SessionLookupSource::Codex,
            Self::Cursor => SessionLookupSource::Cursor,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedSessionTarget {
    pub(crate) source: SessionTargetSource,
    pub(crate) session_id: Option<String>,
}

pub(crate) fn parse_session_target_args(
    targets: Vec<String>,
    action: &str,
) -> anyhow::Result<ParsedSessionTarget> {
    match targets.as_slice() {
        [] => Ok(ParsedSessionTarget {
            source: SessionTargetSource::Praxis,
            session_id: None,
        }),
        [single] => {
            if let Some(source) = SessionTargetSource::from_keyword(single) {
                Ok(ParsedSessionTarget {
                    source,
                    session_id: None,
                })
            } else {
                Ok(ParsedSessionTarget {
                    source: SessionTargetSource::Praxis,
                    session_id: Some(single.clone()),
                })
            }
        }
        [source, session_id] => {
            let Some(source) = SessionTargetSource::from_keyword(source) else {
                anyhow::bail!(
                    "`praxis {action}` accepts two positional arguments only when the first is `praxis`, `codex`, or `cursor`."
                );
            };
            Ok(ParsedSessionTarget {
                source,
                session_id: Some(session_id.clone()),
            })
        }
        _ => unreachable!("clap limits session target args to at most two values"),
    }
}

pub(crate) fn collect_session_target_args(
    target: Option<String>,
    target_extra: Option<String>,
) -> Vec<String> {
    target.into_iter().chain(target_extra).collect()
}

pub(crate) fn validate_session_target_with_last(
    parsed_target: &ParsedSessionTarget,
    last: bool,
    action: &str,
) -> anyhow::Result<()> {
    if last && parsed_target.session_id.is_some() {
        anyhow::bail!(
            "`praxis {action} --last` cannot be combined with an explicit session id or thread name."
        );
    }
    Ok(())
}
