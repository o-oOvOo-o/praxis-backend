use crate::model::RedactedText;
use anyhow::Result;
use anyhow::bail;
use regex::Regex;
use std::collections::BTreeSet;

struct Rule {
    kind: &'static str,
    pattern: &'static str,
    replacement: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        kind: "bearer-token",
        pattern: r"(?i)(authorization\s*:\s*bearer\s+)[A-Za-z0-9._~+/=-]{8,}",
        replacement: "$1[REDACTED:bearer-token]",
    },
    Rule {
        kind: "secret",
        pattern: r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{16,})\b",
        replacement: "[REDACTED:secret]",
    },
    Rule {
        kind: "secret",
        pattern: r#"(?im)([A-Z][A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|API_KEY)[A-Z0-9_]*\s*=\s*)(?:"[^"\r\n]*"|'[^'\r\n]*'|[^\s\r\n]+)"#,
        replacement: "$1[REDACTED:secret]",
    },
    Rule {
        kind: "url-credentials",
        pattern: r"(?i)\bhttps?://[^/\s:@]+:[^@\s/]+@",
        replacement: "https://[REDACTED:url-credentials]@",
    },
    Rule {
        kind: "secret",
        pattern: r"(?i)([?&](?:access_token|token|api_key|key|secret)=)[^&#\s]+",
        replacement: "$1[REDACTED:secret]",
    },
    Rule {
        kind: "email",
        pattern: r"\b[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b",
        replacement: "[REDACTED:email]",
    },
    Rule {
        kind: "absolute-path",
        pattern: r#"(?i)(^|[\s("'=])/?[A-Z]:[/\\][^\s\r\n`"'<>|)\]]*"#,
        replacement: "$1[REDACTED:absolute-path]",
    },
    Rule {
        kind: "absolute-path",
        pattern: r#"/(?:home|Users)/[^\s\r\n`"'<>|]+"#,
        replacement: "[REDACTED:absolute-path]",
    },
];

pub fn redact_text(input: &str) -> Result<RedactedText> {
    let mut text = input.to_string();
    let mut count = 0;
    let mut kinds = BTreeSet::new();

    for rule in RULES {
        let regex = Regex::new(rule.pattern)?;
        let matches = regex.find_iter(&text).count();
        if matches == 0 {
            continue;
        }
        text = regex.replace_all(&text, rule.replacement).into_owned();
        count += matches;
        kinds.insert(rule.kind.to_string());
    }

    for pattern in [
        r"(?i)authorization\s*:\s*bearer\s+[A-Za-z0-9._~+/=-]{8,}",
        r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{16,})\b",
        r"(?i)\bhttps?://[^/\s:@]+:[^@\s/]+@",
    ] {
        if Regex::new(pattern)?.is_match(&text) {
            bail!("credential-like content remained after redaction");
        }
    }

    Ok(RedactedText {
        text,
        count,
        kinds: kinds.into_iter().collect(),
    })
}
