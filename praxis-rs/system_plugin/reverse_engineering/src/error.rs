use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ReverseError {
    #[error("reverse engineering authorization failed: {0}")]
    Authorization(String),
    #[error("reverse engineering artifact codec rejected raw exposure: {0}")]
    Codec(String),
    #[error("reverse engineering IO failed at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("reverse engineering JSON failed at {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("reverse engineering registry failed: {source}")]
    Registry {
        #[source]
        source: Box<serde_json::Error>,
    },
    #[error("unsupported reverse engineering tool {tool_name}: {reason}")]
    UnsupportedTool { tool_name: String, reason: String },
}

impl ReverseError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::Json {
            path: path.into(),
            source,
        }
    }
}
