use std::sync::Arc;

use crate::agent_os::AgentOs;
use crate::tools::runtimes::shell::ShellHostProcessCleaner;
use crate::unified_exec::UnifiedExecProcessManager;

pub(in crate::praxis::session_startup) async fn attach_process_cleaners(
    agent_os: &Arc<AgentOs>,
    unified_exec_manager: Arc<UnifiedExecProcessManager>,
) {
    agent_os
        .attach_process_cleaner(Arc::clone(&unified_exec_manager))
        .await;
    agent_os
        .attach_process_cleaner(Arc::new(ShellHostProcessCleaner::shell()))
        .await;
    agent_os
        .attach_process_cleaner(Arc::new(ShellHostProcessCleaner::zsh_fork()))
        .await;
}
