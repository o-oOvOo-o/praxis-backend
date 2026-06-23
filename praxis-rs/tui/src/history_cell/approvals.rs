use super::*;

pub(crate) fn new_approval_decision_cell(
    command: Vec<String>,
    decision: praxis_protocol::protocol::ReviewDecision,
    actor: ApprovalDecisionActor,
) -> Box<dyn HistoryCell> {
    use praxis_protocol::protocol::NetworkPolicyRuleAction;
    use praxis_protocol::protocol::ReviewDecision::*;

    let (symbol, summary): (Span<'static>, Vec<Span<'static>>) = match decision {
        Approved => {
            let snippet = Span::from(exec_snippet(&command)).dim();
            (
                "✔ ".green(),
                vec![
                    actor.subject().into(),
                    "approved".bold(),
                    " Praxis to run ".into(),
                    snippet,
                    " this time".bold(),
                ],
            )
        }
        ApprovedExecpolicyAmendment {
            proposed_execpolicy_amendment,
        } => {
            let snippet = Span::from(exec_snippet(&proposed_execpolicy_amendment.command)).dim();
            (
                "✔ ".green(),
                vec![
                    actor.subject().into(),
                    "approved".bold(),
                    " Praxis to always run commands that start with ".into(),
                    snippet,
                ],
            )
        }
        ApprovedForSession => {
            let snippet = Span::from(exec_snippet(&command)).dim();
            (
                "✔ ".green(),
                vec![
                    actor.subject().into(),
                    "approved".bold(),
                    " Praxis to run ".into(),
                    snippet,
                    " every time this session".bold(),
                ],
            )
        }
        NetworkPolicyAmendment {
            network_policy_amendment,
        } => match network_policy_amendment.action {
            NetworkPolicyRuleAction::Allow => (
                "✔ ".green(),
                vec![
                    actor.subject().into(),
                    "persisted".bold(),
                    " Praxis network access to ".into(),
                    Span::from(network_policy_amendment.host).dim(),
                ],
            ),
            NetworkPolicyRuleAction::Deny => (
                "✗ ".red(),
                vec![
                    actor.subject().into(),
                    "denied".bold(),
                    " Praxis network access to ".into(),
                    Span::from(network_policy_amendment.host).dim(),
                    " and saved that rule".into(),
                ],
            ),
        },
        Denied => {
            let snippet = Span::from(exec_snippet(&command)).dim();
            let summary = match actor {
                ApprovalDecisionActor::User => vec![
                    actor.subject().into(),
                    "did not approve".bold(),
                    " Praxis to run ".into(),
                    snippet,
                ],
                ApprovalDecisionActor::Guardian => vec![
                    "Request ".into(),
                    "denied".bold(),
                    " for Praxis to run ".into(),
                    snippet,
                ],
            };
            ("✗ ".red(), summary)
        }
        Abort => {
            let snippet = Span::from(exec_snippet(&command)).dim();
            (
                "✗ ".red(),
                vec![
                    actor.subject().into(),
                    "canceled".bold(),
                    " the request to run ".into(),
                    snippet,
                ],
            )
        }
    };

    Box::new(PrefixedWrappedHistoryCell::new(
        Line::from(summary),
        symbol,
        "  ",
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApprovalDecisionActor {
    User,
    Guardian,
}

impl ApprovalDecisionActor {
    fn subject(self) -> &'static str {
        match self {
            Self::User => "You ",
            Self::Guardian => "Auto-reviewer ",
        }
    }
}

pub(crate) fn new_guardian_denied_patch_request(files: Vec<String>) -> Box<dyn HistoryCell> {
    let mut summary = vec![
        "Request ".into(),
        "denied".bold(),
        " for Praxis to apply ".into(),
    ];
    if files.len() == 1 {
        summary.push("a patch touching ".into());
        summary.push(Span::from(files[0].clone()).dim());
    } else {
        summary.push("a patch touching ".into());
        summary.push(Span::from(files.len().to_string()).dim());
        summary.push(" files".into());
    }

    Box::new(PrefixedWrappedHistoryCell::new(
        Line::from(summary),
        "✗ ".red(),
        "  ",
    ))
}

pub(crate) fn new_guardian_denied_action_request(summary: String) -> Box<dyn HistoryCell> {
    let line = Line::from(vec![
        "Request ".into(),
        "denied".bold(),
        " for ".into(),
        Span::from(summary).dim(),
    ]);
    Box::new(PrefixedWrappedHistoryCell::new(line, "✗ ".red(), "  "))
}

pub(crate) fn new_guardian_approved_action_request(summary: String) -> Box<dyn HistoryCell> {
    let line = Line::from(vec![
        "Request ".into(),
        "approved".bold(),
        " for ".into(),
        Span::from(summary).dim(),
    ]);
    Box::new(PrefixedWrappedHistoryCell::new(line, "✔ ".green(), "  "))
}

/// Cyan history cell line showing the current review status.
pub(crate) fn new_review_status_line(message: String) -> PlainHistoryCell {
    PlainHistoryCell {
        lines: vec![Line::from(message.cyan())],
    }
}
