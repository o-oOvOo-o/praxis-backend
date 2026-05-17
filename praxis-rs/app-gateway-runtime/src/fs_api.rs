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
use praxis_exec_server::Environment;
use praxis_exec_server::FsJsonRpcHandler;

#[derive(Clone)]
pub(crate) struct FsApi {
    inner: FsJsonRpcHandler,
}

impl Default for FsApi {
    fn default() -> Self {
        Self {
            inner: FsJsonRpcHandler::new(Environment::default().get_filesystem()),
        }
    }
}

impl FsApi {
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
