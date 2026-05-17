use std::sync::Arc;

use praxis_protocol::fs::FsCopyParams;
use praxis_protocol::fs::FsCopyResponse;
use praxis_protocol::fs::FsCreateDirectoryParams;
use praxis_protocol::fs::FsCreateDirectoryResponse;
use praxis_protocol::fs::FsGetMetadataParams;
use praxis_protocol::fs::FsGetMetadataResponse;
use praxis_protocol::fs::FsReadDirectoryParams;
use praxis_protocol::fs::FsReadDirectoryResponse;
use praxis_protocol::fs::FsReadFileParams;
use praxis_protocol::fs::FsReadFileResponse;
use praxis_protocol::fs::FsRemoveParams;
use praxis_protocol::fs::FsRemoveResponse;
use praxis_protocol::fs::FsWriteFileParams;
use praxis_protocol::fs::FsWriteFileResponse;
use praxis_protocol::jsonrpc_lite::JSONRPCErrorError;

use crate::ExecutorFileSystem;
use crate::FsJsonRpcHandler;
use crate::local_file_system::LocalFileSystem;

#[derive(Clone)]
pub(crate) struct FileSystemHandler {
    inner: FsJsonRpcHandler,
}

impl Default for FileSystemHandler {
    fn default() -> Self {
        let file_system: Arc<dyn ExecutorFileSystem> = Arc::new(LocalFileSystem);
        Self {
            inner: FsJsonRpcHandler::new(file_system),
        }
    }
}

impl FileSystemHandler {
    pub(crate) async fn read_file(
        &self,
        params: FsReadFileParams,
    ) -> Result<FsReadFileResponse, JSONRPCErrorError> {
        self.inner.read_file(params).await
    }

    pub(crate) async fn write_file(
        &self,
        params: FsWriteFileParams,
    ) -> Result<FsWriteFileResponse, JSONRPCErrorError> {
        self.inner.write_file(params).await
    }

    pub(crate) async fn create_directory(
        &self,
        params: FsCreateDirectoryParams,
    ) -> Result<FsCreateDirectoryResponse, JSONRPCErrorError> {
        self.inner.create_directory(params).await
    }

    pub(crate) async fn get_metadata(
        &self,
        params: FsGetMetadataParams,
    ) -> Result<FsGetMetadataResponse, JSONRPCErrorError> {
        self.inner.get_metadata(params).await
    }

    pub(crate) async fn read_directory(
        &self,
        params: FsReadDirectoryParams,
    ) -> Result<FsReadDirectoryResponse, JSONRPCErrorError> {
        self.inner.read_directory(params).await
    }

    pub(crate) async fn remove(
        &self,
        params: FsRemoveParams,
    ) -> Result<FsRemoveResponse, JSONRPCErrorError> {
        self.inner.remove(params).await
    }

    pub(crate) async fn copy(
        &self,
        params: FsCopyParams,
    ) -> Result<FsCopyResponse, JSONRPCErrorError> {
        self.inner.copy(params).await
    }
}
