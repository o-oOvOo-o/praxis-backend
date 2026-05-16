use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use praxis_app_gateway_protocol::FsCopyParams;
use praxis_app_gateway_protocol::FsCopyResponse;
use praxis_app_gateway_protocol::FsCreateDirectoryParams;
use praxis_app_gateway_protocol::FsCreateDirectoryResponse;
use praxis_app_gateway_protocol::FsGetMetadataParams;
use praxis_app_gateway_protocol::FsGetMetadataResponse;
use praxis_app_gateway_protocol::FsReadDirectoryEntry;
use praxis_app_gateway_protocol::FsReadDirectoryParams;
use praxis_app_gateway_protocol::FsReadDirectoryResponse;
use praxis_app_gateway_protocol::FsReadFileParams;
use praxis_app_gateway_protocol::FsReadFileResponse;
use praxis_app_gateway_protocol::FsRemoveParams;
use praxis_app_gateway_protocol::FsRemoveResponse;
use praxis_app_gateway_protocol::FsWriteFileParams;
use praxis_app_gateway_protocol::FsWriteFileResponse;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_exec_server::CopyOptions;
use praxis_exec_server::CreateDirectoryOptions;
use praxis_exec_server::Environment;
use praxis_exec_server::ExecutorFileSystem;
use praxis_exec_server::RemoveOptions;
use std::io;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct FsApi {
    file_system: Arc<dyn ExecutorFileSystem>,
}

impl Default for FsApi {
    fn default() -> Self {
        Self {
            file_system: Environment::default().get_filesystem(),
        }
    }
}

impl FsApi {
    pub(crate) async fn read_file(
        &self,
        params: FsReadFileParams,
    ) -> Result<FsReadFileResponse, JSONRPCErrorError> {
        let bytes = self
            .file_system
            .read_file(&params.path)
            .await
            .map_err(map_fs_error)?;
        Ok(FsReadFileResponse {
            data_base64: STANDARD.encode(bytes),
        })
    }

    pub(crate) async fn write_file(
        &self,
        params: FsWriteFileParams,
    ) -> Result<FsWriteFileResponse, JSONRPCErrorError> {
        let bytes = STANDARD.decode(params.data_base64).map_err(|err| {
            invalid_request(format!(
                "fs/writeFile requires valid base64 dataBase64: {err}"
            ))
        })?;
        self.file_system
            .write_file(&params.path, bytes)
            .await
            .map_err(map_fs_error)?;
        Ok(FsWriteFileResponse {})
    }

    pub(crate) async fn create_directory(
        &self,
        params: FsCreateDirectoryParams,
    ) -> Result<FsCreateDirectoryResponse, JSONRPCErrorError> {
        self.file_system
            .create_directory(
                &params.path,
                CreateDirectoryOptions {
                    recursive: params.recursive.unwrap_or(true),
                },
            )
            .await
            .map_err(map_fs_error)?;
        Ok(FsCreateDirectoryResponse {})
    }

    pub(crate) async fn get_metadata(
        &self,
        params: FsGetMetadataParams,
    ) -> Result<FsGetMetadataResponse, JSONRPCErrorError> {
        let metadata = self
            .file_system
            .get_metadata(&params.path)
            .await
            .map_err(map_fs_error)?;
        Ok(FsGetMetadataResponse {
            is_directory: metadata.is_directory,
            is_file: metadata.is_file,
            created_at_ms: metadata.created_at_ms,
            modified_at_ms: metadata.modified_at_ms,
        })
    }

    pub(crate) async fn read_directory(
        &self,
        params: FsReadDirectoryParams,
    ) -> Result<FsReadDirectoryResponse, JSONRPCErrorError> {
        let entries = self
            .file_system
            .read_directory(&params.path)
            .await
            .map_err(map_fs_error)?;
        Ok(FsReadDirectoryResponse {
            entries: entries
                .into_iter()
                .map(|entry| FsReadDirectoryEntry {
                    file_name: entry.file_name,
                    is_directory: entry.is_directory,
                    is_file: entry.is_file,
                })
                .collect(),
        })
    }

    pub(crate) async fn remove(
        &self,
        params: FsRemoveParams,
    ) -> Result<FsRemoveResponse, JSONRPCErrorError> {
        self.file_system
            .remove(
                &params.path,
                RemoveOptions {
                    recursive: params.recursive.unwrap_or(true),
                    force: params.force.unwrap_or(true),
                },
            )
            .await
            .map_err(map_fs_error)?;
        Ok(FsRemoveResponse {})
    }

    pub(crate) async fn copy(
        &self,
        params: FsCopyParams,
    ) -> Result<FsCopyResponse, JSONRPCErrorError> {
        self.file_system
            .copy(
                &params.source_path,
                &params.destination_path,
                CopyOptions {
                    recursive: params.recursive,
                },
            )
            .await
            .map_err(map_fs_error)?;
        Ok(FsCopyResponse {})
    }
}

pub(crate) fn invalid_request(message: impl Into<String>) -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: INVALID_REQUEST_ERROR_CODE,
        message: message.into(),
        data: None,
    }
}

pub(crate) fn map_fs_error(err: io::Error) -> JSONRPCErrorError {
    if err.kind() == io::ErrorKind::InvalidInput {
        invalid_request(err.to_string())
    } else {
        JSONRPCErrorError {
            code: INTERNAL_ERROR_CODE,
            message: err.to_string(),
            data: None,
        }
    }
}
