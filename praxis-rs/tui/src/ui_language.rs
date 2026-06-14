#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum UiLanguage {
    #[default]
    En,
    Cn,
}

impl UiLanguage {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" | "eng" | "english" => Some(Self::En),
            "cn" | "zh" | "zh-cn" | "chinese" | "中文" => Some(Self::Cn),
            _ => None,
        }
    }

    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::En => Self::Cn,
            Self::Cn => Self::En,
        }
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Cn => "中文",
        }
    }

    pub(crate) fn language_changed_message(self) -> String {
        match self {
            Self::En => "Language switched to English.".to_string(),
            Self::Cn => "语言已切换为中文。".to_string(),
        }
    }

    pub(crate) fn language_status_message(self) -> String {
        match self {
            Self::En => "Current language: English.".to_string(),
            Self::Cn => "当前语言：中文。".to_string(),
        }
    }

    pub(crate) fn language_usage_message(self) -> &'static str {
        match self {
            Self::En => "Usage: /language [en|cn|toggle|status]",
            Self::Cn => "用法：/language [en|cn|toggle|status]",
        }
    }

    pub(crate) fn invalid_language_message(self, value: &str) -> String {
        match self {
            Self::En => {
                format!("Unsupported language `{value}`. Usage: /language [en|cn|toggle|status]")
            }
            Self::Cn => format!("不支持的语言 `{value}`。用法：/language [en|cn|toggle|status]"),
        }
    }

    pub(crate) fn workspace_title(self) -> &'static str {
        match self {
            Self::En => " Praxis ",
            Self::Cn => " Praxis ",
        }
    }

    pub(crate) fn workspace_thread_count(self, count: usize) -> String {
        match self {
            Self::En => format!("{count} threads"),
            Self::Cn => format!("{count} 个线程"),
        }
    }

    pub(crate) fn workspace_new_thread(self) -> &'static str {
        match self {
            Self::En => "+ New",
            Self::Cn => "+ 新建",
        }
    }

    pub(crate) fn workspace_search_placeholder(self) -> &'static str {
        match self {
            Self::En => "Search threads",
            Self::Cn => "搜索线程",
        }
    }

    pub(crate) fn workspace_load_more_threads(self) -> &'static str {
        match self {
            Self::En => "Load more",
            Self::Cn => "加载更多",
        }
    }

    pub(crate) fn workspace_loading_more_threads(self) -> &'static str {
        match self {
            Self::En => "Loading...",
            Self::Cn => "加载中...",
        }
    }

    pub(crate) fn workspace_loaded_threads(self, count: usize) -> String {
        match self {
            Self::En => format!("{count} loaded"),
            Self::Cn => format!("已加载 {count} 个"),
        }
    }

    pub(crate) fn workspace_locked_view(self) -> &'static str {
        match self {
            Self::En => "LOCKED VIEW",
            Self::Cn => "只读锁定",
        }
    }

    pub(crate) fn workspace_controlled(self) -> &'static str {
        match self {
            Self::En => "CONTROLLED",
            Self::Cn => "被接管",
        }
    }

    pub(crate) fn workspace_controller_kind(self, thread_controller: bool) -> &'static str {
        match (self, thread_controller) {
            (Self::En, true) => "agent group",
            (Self::En, false) => "external",
            (Self::Cn, true) => "线程组",
            (Self::Cn, false) => "外部",
        }
    }

    pub(crate) fn workspace_control_mode(self, read_only: bool) -> &'static str {
        match (self, read_only) {
            (Self::En, true) => "locked",
            (Self::En, false) => "controlled",
            (Self::Cn, true) => "只读锁定",
            (Self::Cn, false) => "接管",
        }
    }

    pub(crate) fn workspace_control_by(self) -> &'static str {
        match self {
            Self::En => "by",
            Self::Cn => "由",
        }
    }

    pub(crate) fn workspace_context_title(self) -> &'static str {
        match self {
            Self::En => " Thread ",
            Self::Cn => " 线程 ",
        }
    }

    pub(crate) fn workspace_rename_title(self) -> &'static str {
        match self {
            Self::En => " Rename ",
            Self::Cn => " 重命名 ",
        }
    }

    pub(crate) fn workspace_thread_name_label(self) -> &'static str {
        match self {
            Self::En => "Thread name",
            Self::Cn => "线程名称",
        }
    }

    pub(crate) fn workspace_save_label(self) -> &'static str {
        match self {
            Self::En => " Save ",
            Self::Cn => " 保存 ",
        }
    }

    pub(crate) fn workspace_cancel_label(self) -> &'static str {
        match self {
            Self::En => " Cancel ",
            Self::Cn => " 取消 ",
        }
    }

    pub(crate) fn workspace_archive_title(self) -> &'static str {
        match self {
            Self::En => " Archive ",
            Self::Cn => " 归档 ",
        }
    }

    pub(crate) fn workspace_delete_title(self) -> &'static str {
        match self {
            Self::En => " Delete ",
            Self::Cn => " 删除 ",
        }
    }

    pub(crate) fn workspace_archive_prompt(self, name: &str) -> String {
        match self {
            Self::En => format!("Archive {name}?"),
            Self::Cn => format!("归档 {name}？"),
        }
    }

    pub(crate) fn workspace_delete_prompt(self, name: &str) -> String {
        match self {
            Self::En => format!("Delete {name}?"),
            Self::Cn => format!("删除 {name}？"),
        }
    }
}
