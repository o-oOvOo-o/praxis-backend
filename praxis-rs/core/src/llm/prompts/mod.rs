#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptLayer {
    Product,
    Profile,
    Common,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LlmPromptPurpose {
    ModelInstructions,
    AutoTitle,
    AutoSummary,
    Compact,
}

impl LlmPromptPurpose {
    pub(crate) fn slots(self) -> &'static [&'static str] {
        match self {
            Self::ModelInstructions => &[
                "base",
                "system",
                "instructions",
                "model_instructions",
                "modelInstructions",
            ],
            Self::AutoTitle => &[
                "auto_title",
                "autoTitle",
                "title",
                "task.title",
                "task_title",
                "tasks.title",
            ],
            Self::AutoSummary => &[
                "auto_summary",
                "autoSummary",
                "summary",
                "task.summary",
                "task_summary",
                "tasks.summary",
            ],
            Self::Compact => &[
                "compact",
                "compact_summary",
                "compactSummary",
                "task.compact",
                "task_compact",
                "tasks.compact",
            ],
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ModelInstructions => "model_instructions",
            Self::AutoTitle => "auto_title",
            Self::AutoSummary => "auto_summary",
            Self::Compact => "compact",
        }
    }
}
