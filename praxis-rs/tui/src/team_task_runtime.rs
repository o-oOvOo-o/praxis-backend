//! Team-task cache and per-thread task summary helpers.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::collections::HashMap;

use praxis_app_server_protocol::Team;
use praxis_app_server_protocol::TeamDeletedNotification;
use praxis_app_server_protocol::TeamTask;
use praxis_app_server_protocol::TeamTaskStatus;
use praxis_app_server_protocol::TeamTaskUpdatedNotification;
use praxis_app_server_protocol::TeamTeammate;
use praxis_app_server_protocol::TeamTeammateUpdatedNotification;
use praxis_app_server_protocol::TeamUpdatedNotification;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadTeamTaskSummary {
    pub(crate) team_id: String,
    pub(crate) team_name: String,
    pub(crate) viewed_teammate_id: Option<String>,
    pub(crate) teammate_count: usize,
    pub(crate) in_progress_count: usize,
    pub(crate) pending_count: usize,
    pub(crate) blocked_count: usize,
    pub(crate) current_task: Option<ThreadTeamTaskItem>,
    pub(crate) next_task: Option<ThreadTeamTaskItem>,
    pub(crate) queued_tasks: Vec<ThreadTeamTaskItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadTeamTaskItem {
    pub(crate) task_id: String,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) status: TeamTaskStatus,
    pub(crate) assignee_teammate_id: Option<String>,
    pub(crate) assignee_name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TeamTaskRuntime {
    teams: HashMap<String, TeamRuntime>,
}

#[derive(Clone, Debug, PartialEq)]
struct TeamRuntime {
    team: Team,
    teammates: HashMap<String, TeamTeammate>,
    tasks: BTreeMap<String, TeamTask>,
}

const QUEUED_TASK_PREVIEW_LIMIT: usize = 2;

impl TeamTaskRuntime {
    pub(crate) fn apply_team_updated_notification(
        &mut self,
        notification: TeamUpdatedNotification,
    ) -> bool {
        let team_id = notification.team.id.clone();
        match self.teams.get_mut(&team_id) {
            Some(existing) => {
                if existing.team == notification.team {
                    false
                } else {
                    existing.team = notification.team;
                    true
                }
            }
            None => {
                self.teams.insert(
                    team_id,
                    TeamRuntime {
                        team: notification.team,
                        teammates: HashMap::new(),
                        tasks: BTreeMap::new(),
                    },
                );
                true
            }
        }
    }

    pub(crate) fn apply_team_deleted_notification(
        &mut self,
        notification: TeamDeletedNotification,
    ) -> bool {
        self.teams.remove(&notification.team_id).is_some()
    }

    pub(crate) fn apply_teammate_updated_notification(
        &mut self,
        notification: TeamTeammateUpdatedNotification,
    ) -> bool {
        let team = self
            .teams
            .entry(notification.team_id.clone())
            .or_insert_with(|| TeamRuntime {
                team: Team {
                    id: notification.team_id.clone(),
                    lead_thread_id: String::new(),
                    name: notification.team_id.clone(),
                    objective: None,
                    execution_mode: praxis_app_server_protocol::TeamExecutionMode::ProcessFirst,
                    resume_mode: praxis_app_server_protocol::TeamResumeMode::StrongResume,
                    created_at: notification.teammate.created_at,
                    updated_at: notification.teammate.updated_at,
                },
                teammates: HashMap::new(),
                tasks: BTreeMap::new(),
            });

        match team.teammates.insert(
            notification.teammate.teammate_id.clone(),
            notification.teammate,
        ) {
            Some(previous) => {
                previous
                    != *team
                        .teammates
                        .get(&previous.teammate_id)
                        .expect("teammate should exist after insert")
            }
            None => true,
        }
    }

    pub(crate) fn apply_task_updated_notification(
        &mut self,
        notification: TeamTaskUpdatedNotification,
    ) -> bool {
        let team = self
            .teams
            .entry(notification.team_id.clone())
            .or_insert_with(|| TeamRuntime {
                team: Team {
                    id: notification.team_id.clone(),
                    lead_thread_id: String::new(),
                    name: notification.team_id.clone(),
                    objective: None,
                    execution_mode: praxis_app_server_protocol::TeamExecutionMode::ProcessFirst,
                    resume_mode: praxis_app_server_protocol::TeamResumeMode::StrongResume,
                    created_at: notification.task.created_at,
                    updated_at: notification.task.updated_at,
                },
                teammates: HashMap::new(),
                tasks: BTreeMap::new(),
            });

        match team
            .tasks
            .insert(notification.task.task_id.clone(), notification.task)
        {
            Some(previous) => {
                previous
                    != *team
                        .tasks
                        .get(&previous.task_id)
                        .expect("task should exist after insert")
            }
            None => true,
        }
    }

    pub(crate) fn summary_for_thread(&self, thread_id: &str) -> Option<ThreadTeamTaskSummary> {
        let (team_id, team) = self.team_for_thread(thread_id)?;
        let viewed_teammate_id = preferred_teammate_id(team, thread_id);
        let current_task = choose_current_task(team, thread_id);
        let next_task = choose_next_task(team, thread_id);
        let in_progress_count = team
            .tasks
            .values()
            .filter(|task| task.status == TeamTaskStatus::InProgress)
            .count();
        let pending_count = team
            .tasks
            .values()
            .filter(|task| task.status == TeamTaskStatus::Pending)
            .count();
        let blocked_count = team
            .tasks
            .values()
            .filter(|task| task.status == TeamTaskStatus::Blocked)
            .count();
        let queued_tasks =
            queued_preview_tasks(team, thread_id, next_task.map(|task| task.task_id.as_str()));
        Some(ThreadTeamTaskSummary {
            team_id: team_id.to_string(),
            team_name: team.team.name.clone(),
            viewed_teammate_id: viewed_teammate_id.clone(),
            teammate_count: team.teammates.len(),
            in_progress_count,
            pending_count,
            blocked_count,
            current_task: current_task.map(|task| task_to_item(team, task)),
            next_task: next_task.map(|task| task_to_item(team, task)),
            queued_tasks: queued_tasks
                .into_iter()
                .map(|task| task_to_item(team, task))
                .collect(),
        })
    }

    fn team_for_thread(&self, thread_id: &str) -> Option<(&str, &TeamRuntime)> {
        self.teams.iter().find_map(|(team_id, team)| {
            let is_lead = team.team.lead_thread_id == thread_id;
            let is_teammate = team
                .teammates
                .values()
                .any(|teammate| teammate.thread_id.as_deref() == Some(thread_id));
            (is_lead || is_teammate).then_some((team_id.as_str(), team))
        })
    }
}

fn choose_current_task<'a>(team: &'a TeamRuntime, thread_id: &str) -> Option<&'a TeamTask> {
    let preferred_teammate = preferred_teammate_id(team, thread_id);
    team.tasks
        .values()
        .filter(|task| task.status == TeamTaskStatus::InProgress)
        .max_by_key(|task| {
            (
                task.assignee_teammate_id == preferred_teammate,
                task.updated_at,
                Reverse(task.created_at),
                task.task_id.clone(),
            )
        })
}

fn choose_next_task<'a>(team: &'a TeamRuntime, thread_id: &str) -> Option<&'a TeamTask> {
    sorted_pending_tasks(team, thread_id).into_iter().next()
}

fn sorted_pending_tasks<'a>(team: &'a TeamRuntime, thread_id: &str) -> Vec<&'a TeamTask> {
    let preferred_teammate = preferred_teammate_id(team, thread_id);
    let preferred_teammate = preferred_teammate.as_deref();
    let mut tasks = team
        .tasks
        .values()
        .filter(|task| task.status == TeamTaskStatus::Pending)
        .collect::<Vec<_>>();
    tasks.sort_by_key(|task| {
        (
            task.assignee_teammate_id.as_deref() != preferred_teammate,
            task.created_at,
            task.updated_at,
            task.task_id.clone(),
        )
    });
    tasks
}

fn queued_preview_tasks<'a>(
    team: &'a TeamRuntime,
    thread_id: &str,
    next_task_id: Option<&str>,
) -> Vec<&'a TeamTask> {
    sorted_pending_tasks(team, thread_id)
        .into_iter()
        .filter(|task| Some(task.task_id.as_str()) != next_task_id)
        .take(QUEUED_TASK_PREVIEW_LIMIT)
        .collect()
}

fn preferred_teammate_id(team: &TeamRuntime, thread_id: &str) -> Option<String> {
    team.teammates.values().find_map(|teammate| {
        (teammate.thread_id.as_deref() == Some(thread_id)).then(|| teammate.teammate_id.clone())
    })
}

fn task_to_item(team: &TeamRuntime, task: &TeamTask) -> ThreadTeamTaskItem {
    ThreadTeamTaskItem {
        task_id: task.task_id.clone(),
        title: task.title.clone(),
        description: task.description.clone(),
        status: task.status,
        assignee_teammate_id: task.assignee_teammate_id.clone(),
        assignee_name: task
            .assignee_teammate_id
            .as_ref()
            .and_then(|teammate_id| team.teammates.get(teammate_id))
            .map(|teammate| teammate.name.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use praxis_app_server_protocol::TeamExecutionMode;
    use praxis_app_server_protocol::TeamResumeMode;
    use praxis_app_server_protocol::TeamTeammateStatus;
    use pretty_assertions::assert_eq;

    fn team(team_id: &str, lead_thread_id: &str) -> Team {
        Team {
            id: team_id.to_string(),
            lead_thread_id: lead_thread_id.to_string(),
            name: "Team".to_string(),
            objective: None,
            execution_mode: TeamExecutionMode::ProcessFirst,
            resume_mode: TeamResumeMode::StrongResume,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn teammate(team_id: &str, teammate_id: &str, thread_id: &str) -> TeamTeammate {
        TeamTeammate {
            team_id: team_id.to_string(),
            teammate_id: teammate_id.to_string(),
            name: teammate_id.to_string(),
            role: None,
            status: TeamTeammateStatus::Active,
            thread_id: Some(thread_id.to_string()),
            last_error: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn task(
        team_id: &str,
        task_id: &str,
        title: &str,
        status: TeamTaskStatus,
        assignee_teammate_id: Option<&str>,
        created_at: i64,
        updated_at: i64,
    ) -> TeamTask {
        TeamTask {
            team_id: team_id.to_string(),
            task_id: task_id.to_string(),
            title: title.to_string(),
            description: None,
            status,
            assignee_teammate_id: assignee_teammate_id.map(str::to_string),
            created_at,
            updated_at,
            completed_at: None,
        }
    }

    #[test]
    fn summary_for_lead_thread_prefers_in_progress_and_oldest_pending() {
        let mut runtime = TeamTaskRuntime::default();
        assert!(
            runtime.apply_team_updated_notification(TeamUpdatedNotification {
                team: team("team-1", "lead-thread"),
            })
        );
        assert!(
            runtime.apply_task_updated_notification(TeamTaskUpdatedNotification {
                team_id: "team-1".to_string(),
                task: task(
                    "team-1",
                    "2",
                    "Review diff",
                    TeamTaskStatus::Pending,
                    None,
                    2,
                    2
                ),
            })
        );
        assert!(
            runtime.apply_task_updated_notification(TeamTaskUpdatedNotification {
                team_id: "team-1".to_string(),
                task: task(
                    "team-1",
                    "1",
                    "Apply patch",
                    TeamTaskStatus::InProgress,
                    None,
                    1,
                    3,
                ),
            })
        );

        assert_eq!(
            runtime.summary_for_thread("lead-thread"),
            Some(ThreadTeamTaskSummary {
                team_id: "team-1".to_string(),
                team_name: "Team".to_string(),
                viewed_teammate_id: None,
                teammate_count: 0,
                in_progress_count: 1,
                pending_count: 1,
                blocked_count: 0,
                current_task: Some(ThreadTeamTaskItem {
                    task_id: "1".to_string(),
                    title: "Apply patch".to_string(),
                    description: None,
                    status: TeamTaskStatus::InProgress,
                    assignee_teammate_id: None,
                    assignee_name: None,
                }),
                next_task: Some(ThreadTeamTaskItem {
                    task_id: "2".to_string(),
                    title: "Review diff".to_string(),
                    description: None,
                    status: TeamTaskStatus::Pending,
                    assignee_teammate_id: None,
                    assignee_name: None,
                }),
                queued_tasks: Vec::new(),
            })
        );
    }

    #[test]
    fn teammate_thread_prefers_assigned_tasks() {
        let mut runtime = TeamTaskRuntime::default();
        assert!(
            runtime.apply_team_updated_notification(TeamUpdatedNotification {
                team: team("team-1", "lead-thread"),
            })
        );
        assert!(
            runtime.apply_teammate_updated_notification(TeamTeammateUpdatedNotification {
                team_id: "team-1".to_string(),
                teammate: teammate("team-1", "teammate-1", "worker-thread"),
                thread: None,
            })
        );
        assert!(
            runtime.apply_task_updated_notification(TeamTaskUpdatedNotification {
                team_id: "team-1".to_string(),
                task: task(
                    "team-1",
                    "1",
                    "Assigned pending",
                    TeamTaskStatus::Pending,
                    Some("teammate-1"),
                    2,
                    2,
                ),
            })
        );
        assert!(
            runtime.apply_task_updated_notification(TeamTaskUpdatedNotification {
                team_id: "team-1".to_string(),
                task: task(
                    "team-1",
                    "2",
                    "Unassigned pending",
                    TeamTaskStatus::Pending,
                    None,
                    1,
                    1,
                ),
            })
        );

        let summary = runtime
            .summary_for_thread("worker-thread")
            .expect("worker thread summary should exist");
        assert_eq!(summary.team_name, "Team".to_string());
        assert_eq!(summary.viewed_teammate_id, Some("teammate-1".to_string()));
        assert_eq!(summary.teammate_count, 1);
        assert_eq!(summary.in_progress_count, 0);
        assert_eq!(summary.pending_count, 2);
        assert_eq!(summary.blocked_count, 0);
        assert_eq!(
            summary.next_task.expect("next task should exist").task_id,
            "1".to_string()
        );
        assert_eq!(summary.queued_tasks.len(), 1);
        assert_eq!(summary.queued_tasks[0].task_id, "2".to_string());
    }

    #[test]
    fn summary_resolves_assignee_name_for_lead_thread() {
        let mut runtime = TeamTaskRuntime::default();
        assert!(
            runtime.apply_team_updated_notification(TeamUpdatedNotification {
                team: team("team-1", "lead-thread"),
            })
        );
        assert!(
            runtime.apply_teammate_updated_notification(TeamTeammateUpdatedNotification {
                team_id: "team-1".to_string(),
                teammate: teammate("team-1", "teammate-1", "worker-thread"),
                thread: None,
            })
        );
        assert!(
            runtime.apply_task_updated_notification(TeamTaskUpdatedNotification {
                team_id: "team-1".to_string(),
                task: task(
                    "team-1",
                    "1",
                    "Audit diff",
                    TeamTaskStatus::InProgress,
                    Some("teammate-1"),
                    1,
                    2,
                ),
            })
        );

        let summary = runtime
            .summary_for_thread("lead-thread")
            .expect("lead thread summary should exist");
        let current_task = summary
            .current_task
            .expect("current task should exist for lead thread");
        assert_eq!(summary.team_name, "Team".to_string());
        assert_eq!(summary.teammate_count, 1);
        assert_eq!(summary.in_progress_count, 1);
        assert_eq!(summary.pending_count, 0);
        assert_eq!(summary.blocked_count, 0);
        assert!(summary.queued_tasks.is_empty());
        assert_eq!(
            current_task.assignee_teammate_id,
            Some("teammate-1".to_string())
        );
        assert_eq!(current_task.assignee_name, Some("teammate-1".to_string()));
    }

    #[test]
    fn summary_tracks_blocked_tasks() {
        let mut runtime = TeamTaskRuntime::default();
        assert!(
            runtime.apply_team_updated_notification(TeamUpdatedNotification {
                team: team("team-1", "lead-thread"),
            })
        );
        assert!(
            runtime.apply_teammate_updated_notification(TeamTeammateUpdatedNotification {
                team_id: "team-1".to_string(),
                teammate: teammate("team-1", "teammate-1", "worker-thread"),
                thread: None,
            })
        );
        assert!(
            runtime.apply_task_updated_notification(TeamTaskUpdatedNotification {
                team_id: "team-1".to_string(),
                task: task(
                    "team-1",
                    "1",
                    "Audit diff",
                    TeamTaskStatus::InProgress,
                    Some("teammate-1"),
                    1,
                    2,
                ),
            })
        );
        assert!(
            runtime.apply_task_updated_notification(TeamTaskUpdatedNotification {
                team_id: "team-1".to_string(),
                task: task(
                    "team-1",
                    "2",
                    "Wait on approval",
                    TeamTaskStatus::Blocked,
                    None,
                    2,
                    3,
                ),
            })
        );

        let summary = runtime
            .summary_for_thread("lead-thread")
            .expect("lead thread summary should exist");
        assert_eq!(summary.blocked_count, 1);
        assert_eq!(summary.in_progress_count, 1);
        assert_eq!(summary.pending_count, 0);
        assert!(summary.queued_tasks.is_empty());
    }

    #[test]
    fn summary_includes_pending_preview_after_next_task() {
        let mut runtime = TeamTaskRuntime::default();
        assert!(
            runtime.apply_team_updated_notification(TeamUpdatedNotification {
                team: team("team-1", "lead-thread"),
            })
        );
        assert!(
            runtime.apply_teammate_updated_notification(TeamTeammateUpdatedNotification {
                team_id: "team-1".to_string(),
                teammate: teammate("team-1", "teammate-1", "worker-thread"),
                thread: None,
            })
        );
        assert!(
            runtime.apply_task_updated_notification(TeamTaskUpdatedNotification {
                team_id: "team-1".to_string(),
                task: task(
                    "team-1",
                    "1",
                    "First pending",
                    TeamTaskStatus::Pending,
                    None,
                    1,
                    1,
                ),
            })
        );
        assert!(
            runtime.apply_task_updated_notification(TeamTaskUpdatedNotification {
                team_id: "team-1".to_string(),
                task: task(
                    "team-1",
                    "2",
                    "Second pending",
                    TeamTaskStatus::Pending,
                    None,
                    2,
                    2,
                ),
            })
        );
        assert!(
            runtime.apply_task_updated_notification(TeamTaskUpdatedNotification {
                team_id: "team-1".to_string(),
                task: task(
                    "team-1",
                    "3",
                    "Third pending",
                    TeamTaskStatus::Pending,
                    None,
                    3,
                    3,
                ),
            })
        );

        let summary = runtime
            .summary_for_thread("lead-thread")
            .expect("lead thread summary should exist");
        assert_eq!(
            summary
                .next_task
                .as_ref()
                .expect("next task should exist")
                .task_id,
            "1".to_string()
        );
        assert_eq!(
            summary
                .queued_tasks
                .iter()
                .map(|task| task.task_id.as_str())
                .collect::<Vec<_>>(),
            vec!["2", "3"]
        );
    }
}
