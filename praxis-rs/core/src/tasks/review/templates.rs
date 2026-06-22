use std::borrow::Cow;
use std::sync::LazyLock;

use praxis_utils_template::Template;

static REVIEW_EXIT_SUCCESS_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    let normalized =
        normalize_review_template_line_endings(crate::client_common::REVIEW_EXIT_SUCCESS_TMPL);
    Template::parse(normalized.as_ref())
        .unwrap_or_else(|err| panic!("review exit success template must parse: {err}"))
});

pub(super) fn render_review_exit_success(results: &str) -> String {
    REVIEW_EXIT_SUCCESS_TEMPLATE
        .render([("results", results)])
        .unwrap_or_else(|err| panic!("review exit success template must render: {err}"))
}

pub(super) fn normalize_review_template_line_endings(template: &str) -> Cow<'_, str> {
    if template.contains('\r') {
        Cow::Owned(template.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(template)
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_review_template_line_endings;
    use super::render_review_exit_success;
    use pretty_assertions::assert_eq;

    #[test]
    fn render_review_exit_success_replaces_results_placeholder() {
        assert_eq!(
            render_review_exit_success("Finding A\nFinding B"),
            "<user_action>\n  <context>User initiated a review task. Here's the full review output from reviewer model. User may select one or more comments to resolve.</context>\n  <action>review</action>\n  <results>\n  Finding A\nFinding B\n  </results>\n  </user_action>\n"
        );
    }

    #[test]
    fn normalize_review_template_line_endings_rewrites_crlf() {
        assert_eq!(
            normalize_review_template_line_endings("<user_action>\r\n  <results>\r\n  None.\r\n"),
            "<user_action>\n  <results>\n  None.\n"
        );
    }
}
