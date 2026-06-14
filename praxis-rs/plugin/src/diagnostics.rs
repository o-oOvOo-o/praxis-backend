#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDiagnostic {
    pub severity: PluginDiagnosticSeverity,
    pub code: String,
    pub message: String,
}

impl PluginDiagnostic {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: PluginDiagnosticSeverity::Error,
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: PluginDiagnosticSeverity::Warning,
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginDiagnosticSeverity {
    Error,
    Warning,
    Info,
}
