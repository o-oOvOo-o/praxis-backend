use super::*;

use crate::status_runtime::GENERIC_STATUS_HEADER;
use crate::team_task_runtime::ThreadTeamTaskItem;
use crate::team_task_runtime::ThreadTeamTaskSummary;
use crate::turn_runtime::RuntimeTaskSnapshot;
use crate::turn_runtime::RuntimeTextCapitalization;

impl ChatWidget {
    pub(super) fn on_team_updated_notification(
        &mut self,
        notification: praxis_app_server_protocol::TeamUpdatedNotification,
    ) {
        if self
            .team_task_runtime
            .apply_team_updated_notification(notification)
        {
            self.update_task_running_state();
            self.refresh_rendered_status_state();
        }
    }

    pub(super) fn on_team_deleted_notification(
        &mut self,
        notification: praxis_app_server_protocol::TeamDeletedNotification,
    ) {
        if self
            .team_task_runtime
            .apply_team_deleted_notification(notification)
        {
            self.update_task_running_state();
            self.refresh_rendered_status_state();
        }
    }

    pub(super) fn on_team_teammate_updated_notification(
        &mut self,
        notification: praxis_app_server_protocol::TeamTeammateUpdatedNotification,
    ) {
        if self
            .team_task_runtime
            .apply_teammate_updated_notification(notification)
        {
            self.update_task_running_state();
            self.refresh_rendered_status_state();
        }
    }

    pub(super) fn on_team_task_updated_notification(
        &mut self,
        notification: praxis_app_server_protocol::TeamTaskUpdatedNotification,
    ) {
        if self
            .team_task_runtime
            .apply_task_updated_notification(notification)
        {
            self.update_task_running_state();
            self.refresh_rendered_status_state();
        }
    }

    pub(super) fn refresh_rendered_status_state(&mut self) {
        self.turn_status_snapshot.set_base_status(
            self.current_status.header.clone(),
            self.current_status.details.clone(),
            RuntimeTextCapitalization::Preserve,
            self.current_status.details_max_lines,
        );

        let details_override = self.sync_team_task_status();
        let rendered = self.turn_status_snapshot.status_snapshot();
        let details =
            if rendered.details.is_none() && self.current_status.header == GENERIC_STATUS_HEADER {
                details_override
            } else {
                rendered.details.clone()
            };

        self.bottom_pane.update_status(
            rendered.header,
            details,
            StatusDetailsCapitalization::Preserve,
            rendered.details_max_lines,
        );
        self.bottom_pane
            .set_status_activity_message(rendered.activity_message);
        self.bottom_pane.set_status_footer_message(
            (!rendered.extra_lines.is_empty()).then(|| rendered.extra_lines.join("\n")),
        );
        self.refresh_terminal_title();
    }

    fn sync_team_task_status(&mut self) -> Option<String> {
        let Some(summary) = self.current_team_task_summary() else {
            self.turn_status_snapshot.clear_tasks();
            self.turn_status_snapshot.set_tip_message(None);
            self.turn_status_snapshot.set_summary_message(None);
            self.turn_status_snapshot.set_queue_preview_message(None);
            return None;
        };

        let current_description = summary
            .current_task
            .as_ref()
            .and_then(|task| task.description.clone());
        let viewed_teammate_id = summary.viewed_teammate_id.as_deref();
        self.turn_status_snapshot
            .set_summary_message(status_runtime_summary(&summary));
        self.turn_status_snapshot
            .set_queue_preview_message(status_runtime_queue_preview(&summary));
        self.turn_status_snapshot.set_active_task(
            summary
                .current_task
                .as_ref()
                .map(|item| runtime_task_from_item(item, viewed_teammate_id)),
        );
        self.turn_status_snapshot.set_next_task(
            summary
                .next_task
                .as_ref()
                .map(|item| runtime_task_from_item(item, viewed_teammate_id)),
        );
        self.turn_status_snapshot
            .set_tip_message(self.status_runtime_tip(&summary));
        current_description
    }

    fn current_team_task_summary(&self) -> Option<ThreadTeamTaskSummary> {
        let thread_id = self.thread_id.as_ref()?.to_string();
        self.team_task_runtime.summary_for_thread(&thread_id)
    }

    pub(super) fn team_task_running(&self) -> bool {
        self.current_team_task_summary()
            .is_some_and(|summary| summary.current_task.is_some())
    }

    fn status_runtime_tip(&self, summary: &ThreadTeamTaskSummary) -> Option<String> {
        (summary.current_task.is_some() && summary.next_task.is_none())
            .then(|| "Tip: Run /status for the live breakdown".to_string())
    }
}

fn runtime_task_from_item(
    item: &ThreadTeamTaskItem,
    viewed_teammate_id: Option<&str>,
) -> RuntimeTaskSnapshot {
    let (subject, active_form) = runtime_task_labels(item, viewed_teammate_id);
    RuntimeTaskSnapshot::new(item.task_id.clone(), subject, active_form)
}

fn status_runtime_summary(summary: &ThreadTeamTaskSummary) -> Option<String> {
    let is_lead_view = summary.viewed_teammate_id.is_none();
    let extra_active = summary
        .in_progress_count
        .saturating_sub(usize::from(summary.current_task.is_some()));
    let extra_pending = summary
        .pending_count
        .saturating_sub(usize::from(summary.next_task.is_some()));
    let blocked = summary.blocked_count;

    if !is_lead_view && extra_active == 0 && extra_pending == 0 && blocked == 0 {
        return None;
    }

    let mut parts = Vec::new();
    if is_lead_view && summary.teammate_count > 0 {
        parts.push(count_label(summary.teammate_count, "teammate", "teammates"));
    }

    if extra_active > 0 {
        parts.push(format!("{extra_active} more active"));
    }

    if extra_pending > 0 {
        parts.push(format!("{extra_pending} more queued"));
    }

    if blocked > 0 {
        parts.push(format!("{blocked} blocked"));
    }

    if parts.is_empty() {
        return None;
    }

    let team_name = summary.team_name.trim();
    if !team_name.is_empty() {
        parts.insert(0, team_name.to_string());
    }

    Some(parts.join(" · "))
}

fn status_runtime_queue_preview(summary: &ThreadTeamTaskSummary) -> Option<String> {
    if summary.viewed_teammate_id.is_some() || summary.queued_tasks.is_empty() {
        return None;
    }

    let viewed_teammate_id = summary.viewed_teammate_id.as_deref();
    let subjects = summary
        .queued_tasks
        .iter()
        .filter_map(|item| {
            let (subject, _) = runtime_task_labels(item, viewed_teammate_id);
            let subject = subject.trim().to_string();
            (!subject.is_empty()).then_some(subject)
        })
        .collect::<Vec<_>>();
    if subjects.is_empty() {
        return None;
    }

    let previewed_count = usize::from(summary.next_task.is_some()) + summary.queued_tasks.len();
    let remaining_count = summary.pending_count.saturating_sub(previewed_count);

    let mut message = format!("Queue: {}", subjects.join(", "));
    if remaining_count > 0 {
        message.push_str(&format!(" +{remaining_count} more"));
    }

    Some(message)
}

fn runtime_task_labels(
    item: &ThreadTeamTaskItem,
    viewed_teammate_id: Option<&str>,
) -> (String, Option<String>) {
    let title = item.title.trim().to_string();
    let assignee_id = item.assignee_teammate_id.as_deref();
    let assignee_label = item
        .assignee_name
        .as_deref()
        .or(item.assignee_teammate_id.as_deref())
        .map(str::trim)
        .filter(|label| !label.is_empty());

    match (assignee_id, assignee_label) {
        (Some(assignee_id), Some(assignee_label)) if Some(assignee_id) != viewed_teammate_id => (
            format!("{title} ({assignee_label})"),
            Some(format!("{assignee_label}: {title}")),
        ),
        _ => (title, None),
    }
}

fn count_label(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{count} {noun}")
}
