use super::*;

pub(super) fn load_output_schema(path: Option<PathBuf>) -> Option<Value> {
    let path = path?;

    let schema_str = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!(
                "Failed to read output schema file {}: {err}",
                path.display()
            );
            std::process::exit(1);
        }
    };

    match serde_json::from_str::<Value>(&schema_str) {
        Ok(value) => Some(value),
        Err(err) => {
            eprintln!(
                "Output schema file {} is not valid JSON: {err}",
                path.display()
            );
            std::process::exit(1);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PromptDecodeError {
    InvalidUtf8 { valid_up_to: usize },
    InvalidUtf16 { encoding: &'static str },
    UnsupportedBom { encoding: &'static str },
}

impl std::fmt::Display for PromptDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptDecodeError::InvalidUtf8 { valid_up_to } => write!(
                f,
                "input is not valid UTF-8 (invalid byte at offset {valid_up_to}). Convert it to UTF-8 and retry (e.g., `iconv -f <ENC> -t UTF-8 prompt.txt`)."
            ),
            PromptDecodeError::InvalidUtf16 { encoding } => write!(
                f,
                "input looked like {encoding} but could not be decoded. Convert it to UTF-8 and retry."
            ),
            PromptDecodeError::UnsupportedBom { encoding } => write!(
                f,
                "input appears to be {encoding}. Convert it to UTF-8 and retry."
            ),
        }
    }
}

pub(super) fn decode_prompt_bytes(input: &[u8]) -> Result<String, PromptDecodeError> {
    let input = input.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(input);

    if input.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        return Err(PromptDecodeError::UnsupportedBom {
            encoding: "UTF-32LE",
        });
    }

    if input.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return Err(PromptDecodeError::UnsupportedBom {
            encoding: "UTF-32BE",
        });
    }

    if let Some(rest) = input.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, "UTF-16LE", u16::from_le_bytes);
    }

    if let Some(rest) = input.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, "UTF-16BE", u16::from_be_bytes);
    }

    std::str::from_utf8(input)
        .map(str::to_string)
        .map_err(|e| PromptDecodeError::InvalidUtf8 {
            valid_up_to: e.valid_up_to(),
        })
}

pub(super) fn decode_utf16(
    input: &[u8],
    encoding: &'static str,
    decode_unit: fn([u8; 2]) -> u16,
) -> Result<String, PromptDecodeError> {
    if !input.len().is_multiple_of(2) {
        return Err(PromptDecodeError::InvalidUtf16 { encoding });
    }

    let units: Vec<u16> = input
        .chunks_exact(2)
        .map(|chunk| decode_unit([chunk[0], chunk[1]]))
        .collect();

    String::from_utf16(&units).map_err(|_| PromptDecodeError::InvalidUtf16 { encoding })
}

pub(super) fn read_prompt_from_stdin(behavior: StdinPromptBehavior) -> Option<String> {
    let stdin_is_terminal = std::io::stdin().is_terminal();

    match behavior {
        StdinPromptBehavior::RequiredIfPiped if stdin_is_terminal => {
            eprintln!(
                "No prompt provided. Either specify one as an argument or pipe the prompt into stdin."
            );
            std::process::exit(1);
        }
        StdinPromptBehavior::RequiredIfPiped => {
            eprintln!("Reading prompt from stdin...");
        }
        StdinPromptBehavior::Forced => {}
        StdinPromptBehavior::OptionalAppend if stdin_is_terminal => return None,
        StdinPromptBehavior::OptionalAppend => {
            eprintln!("Reading additional input from stdin...");
        }
    }

    let mut bytes = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut bytes) {
        eprintln!("Failed to read prompt from stdin: {e}");
        std::process::exit(1);
    }

    let buffer = match decode_prompt_bytes(&bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read prompt from stdin: {e}");
            std::process::exit(1);
        }
    };

    if buffer.trim().is_empty() {
        match behavior {
            StdinPromptBehavior::OptionalAppend => None,
            StdinPromptBehavior::RequiredIfPiped | StdinPromptBehavior::Forced => {
                eprintln!("No prompt provided via stdin.");
                std::process::exit(1);
            }
        }
    } else {
        Some(buffer)
    }
}

pub(super) fn prompt_with_stdin_context(prompt: &str, stdin_text: &str) -> String {
    let mut combined = format!("{prompt}\n\n<stdin>\n{stdin_text}");
    if !stdin_text.ends_with('\n') {
        combined.push('\n');
    }
    combined.push_str("</stdin>");
    combined
}

pub(super) fn resolve_prompt(prompt_arg: Option<String>) -> String {
    match prompt_arg {
        Some(p) if p != "-" => p,
        maybe_dash => {
            let behavior = if matches!(maybe_dash.as_deref(), Some("-")) {
                StdinPromptBehavior::Forced
            } else {
                StdinPromptBehavior::RequiredIfPiped
            };
            let Some(prompt) = read_prompt_from_stdin(behavior) else {
                unreachable!("required stdin prompt should produce content");
            };
            prompt
        }
    }
}

pub(super) fn resolve_root_prompt(prompt_arg: Option<String>) -> String {
    match prompt_arg {
        Some(prompt) if prompt != "-" => {
            if let Some(stdin_text) = read_prompt_from_stdin(StdinPromptBehavior::OptionalAppend) {
                prompt_with_stdin_context(&prompt, &stdin_text)
            } else {
                prompt
            }
        }
        maybe_dash => resolve_prompt(maybe_dash),
    }
}

pub(super) fn build_review_request(args: &ReviewArgs) -> anyhow::Result<ReviewRequest> {
    let target = if args.uncommitted {
        ReviewTarget::UncommittedChanges
    } else if let Some(branch) = args.base.clone() {
        ReviewTarget::BaseBranch { branch }
    } else if let Some(sha) = args.commit.clone() {
        ReviewTarget::Commit {
            sha,
            title: args.commit_title.clone(),
        }
    } else if let Some(prompt_arg) = args.prompt.clone() {
        let prompt = resolve_prompt(Some(prompt_arg)).trim().to_string();
        if prompt.is_empty() {
            anyhow::bail!("Review prompt cannot be empty");
        }
        ReviewTarget::Custom {
            instructions: prompt,
        }
    } else {
        anyhow::bail!(
            "Specify --uncommitted, --base, --commit, or provide custom review instructions"
        );
    };

    Ok(ReviewRequest {
        target,
        user_facing_hint: None,
    })
}
