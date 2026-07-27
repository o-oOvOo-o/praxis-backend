use std::time::Duration;

use metra_computer_use::{
    ComputerUseChannel, ComputerUseExecution, ComputerUseOrchestrator, ComputerUseRequest,
    ComputerUseResult, ComputerUseSession, ExecutionContext,
};
use metra_computer_use_windows::WindowsComputerUse;
use praxis_utils_approval_presets::PermissionPreset;
use serde_json::Value;

use crate::{PerCallApproval, authorize_computer_use_action, parse_dynamic_tool_call};

pub struct ComputerUseExecutor {
    windows: Option<WindowsComputerUse>,
}

impl ComputerUseExecutor {
    pub const fn host_only() -> Self {
        Self { windows: None }
    }

    pub fn try_with_windows() -> ComputerUseResult<Self> {
        WindowsComputerUse::new().map(|windows| Self {
            windows: Some(windows),
        })
    }

    pub fn with_windows(windows: WindowsComputerUse) -> Self {
        Self {
            windows: Some(windows),
        }
    }

    pub const fn windows_channels_enabled(&self) -> bool {
        self.windows.is_some()
    }

    pub fn execute_dynamic_tool<'channels>(
        &'channels self,
        tool_name: &str,
        arguments: Value,
        permission_preset: PermissionPreset,
        approval: PerCallApproval,
        session: ComputerUseSession,
        host_channels: &[&'channels dyn ComputerUseChannel],
    ) -> ComputerUseResult<ComputerUseExecution> {
        let request = parse_dynamic_tool_call(tool_name, arguments)?;
        self.execute_request(request, permission_preset, approval, session, host_channels)
    }

    pub fn execute_request<'channels>(
        &'channels self,
        request: ComputerUseRequest,
        permission_preset: PermissionPreset,
        approval: PerCallApproval,
        session: ComputerUseSession,
        host_channels: &[&'channels dyn ComputerUseChannel],
    ) -> ComputerUseResult<ComputerUseExecution> {
        request.validate()?;
        authorize_computer_use_action(permission_preset, approval, &request.action)?;

        let context = ExecutionContext::new(session, Duration::from_millis(request.timeout_ms));
        let mut channels = host_channels.to_vec();
        if let Some(windows) = &self.windows {
            channels.extend(windows.channels());
        }
        ComputerUseOrchestrator.execute(&request, &context, &channels)
    }
}

impl Default for ComputerUseExecutor {
    fn default() -> Self {
        Self::host_only()
    }
}
