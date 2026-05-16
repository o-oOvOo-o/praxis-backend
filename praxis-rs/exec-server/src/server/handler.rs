use praxis_app_gateway_protocol::FsCopyParams;
use praxis_app_gateway_protocol::FsCopyResponse;
use praxis_app_gateway_protocol::FsCreateDirectoryParams;
use praxis_app_gateway_protocol::FsCreateDirectoryResponse;
use praxis_app_gateway_protocol::FsGetMetadataParams;
use praxis_app_gateway_protocol::FsGetMetadataResponse;
use praxis_app_gateway_protocol::FsReadDirectoryParams;
use praxis_app_gateway_protocol::FsReadDirectoryResponse;
use praxis_app_gateway_protocol::FsReadFileParams;
use praxis_app_gateway_protocol::FsReadFileResponse;
use praxis_app_gateway_protocol::FsRemoveParams;
use praxis_app_gateway_protocol::FsRemoveResponse;
use praxis_app_gateway_protocol::FsWriteFileParams;
use praxis_app_gateway_protocol::FsWriteFileResponse;
use praxis_app_gateway_protocol::JSONRPCErrorError;

use crate::protocol::ExecParams;
use crate::protocol::ExecResponse;
use crate::protocol::InitializeResponse;
use crate::protocol::ReadParams;
use crate::protocol::ReadResponse;
use crate::protocol::TerminateParams;
use crate::protocol::TerminateResponse;
use crate::protocol::WriteParams;
use crate::protocol::WriteResponse;
use crate::rpc::RpcNotificationSender;
use crate::server::file_system_handler::FileSystemHandler;
use crate::server::process_handler::ProcessHandler;

#[derive(Clone)]
pub(crate) struct ExecServerHandler {
    process: ProcessHandler,
    file_system: FileSystemHandler,
}

impl ExecServerHandler {
    pub(crate) fn new(notifications: RpcNotificationSender) -> Self {
        Self {
            process: ProcessHandler::new(notifications),
            file_system: FileSystemHandler::default(),
        }
    }

    pub(crate) async fn shutdown(&self) {
        self.process.shutdown().await;
    }

    pub(crate) fn initialize(&self) -> Result<InitializeResponse, JSONRPCErrorError> {
        self.process.initialize()
    }

    pub(crate) fn initialized(&self) -> Result<(), String> {
        self.process.initialized()
    }

    pub(crate) async fn exec(&self, params: ExecParams) -> Result<ExecResponse, JSONRPCErrorError> {
        self.process.exec(params).await
    }

    pub(crate) async fn exec_read(
        &self,
        params: ReadParams,
    ) -> Result<ReadResponse, JSONRPCErrorError> {
        self.process.exec_read(params).await
    }

    pub(crate) async fn exec_write(
        &self,
        params: WriteParams,
    ) -> Result<WriteResponse, JSONRPCErrorError> {
        self.process.exec_write(params).await
    }

    pub(crate) async fn terminate(
        &self,
        params: TerminateParams,
    ) -> Result<TerminateResponse, JSONRPCErrorError> {
        self.process.terminate(params).await
    }

    pub(crate) async fn fs_read_file(
        &self,
        params: FsReadFileParams,
    ) -> Result<FsReadFileResponse, JSONRPCErrorError> {
        self.process.require_initialized_for("filesystem")?;
        self.file_system.read_file(params).await
    }

    pub(crate) async fn fs_write_file(
        &self,
        params: FsWriteFileParams,
    ) -> Result<FsWriteFileResponse, JSONRPCErrorError> {
        self.process.require_initialized_for("filesystem")?;
        self.file_system.write_file(params).await
    }

    pub(crate) async fn fs_create_directory(
        &self,
        params: FsCreateDirectoryParams,
    ) -> Result<FsCreateDirectoryResponse, JSONRPCErrorError> {
        self.process.require_initialized_for("filesystem")?;
        self.file_system.create_directory(params).await
    }

    pub(crate) async fn fs_get_metadata(
        &self,
        params: FsGetMetadataParams,
    ) -> Result<FsGetMetadataResponse, JSONRPCErrorError> {
        self.process.require_initialized_for("filesystem")?;
        self.file_system.get_metadata(params).await
    }

    pub(crate) async fn fs_read_directory(
        &self,
        params: FsReadDirectoryParams,
    ) -> Result<FsReadDirectoryResponse, JSONRPCErrorError> {
        self.process.require_initialized_for("filesystem")?;
        self.file_system.read_directory(params).await
    }

    pub(crate) async fn fs_remove(
        &self,
        params: FsRemoveParams,
    ) -> Result<FsRemoveResponse, JSONRPCErrorError> {
        self.process.require_initialized_for("filesystem")?;
        self.file_system.remove(params).await
    }

    pub(crate) async fn fs_copy(
        &self,
        params: FsCopyParams,
    ) -> Result<FsCopyResponse, JSONRPCErrorError> {
        self.process.require_initialized_for("filesystem")?;
        self.file_system.copy(params).await
    }
}

#[cfg(test)]
mod tests;
