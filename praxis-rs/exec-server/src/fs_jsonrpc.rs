use std::io;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use praxis_protocol::fs::FsCopyParams;
use praxis_protocol::fs::FsCopyResponse;
use praxis_protocol::fs::FsCreateDirectoryParams;
use praxis_protocol::fs::FsCreateDirectoryResponse;
use praxis_protocol::fs::FsGetMetadataParams;
use praxis_protocol::fs::FsGetMetadataResponse;
use praxis_protocol::fs::FsReadDirectoryEntry;
use praxis_protocol::fs::FsReadDirectoryParams;
use praxis_protocol::fs::FsReadDirectoryResponse;
use praxis_protocol::fs::FsReadFileParams;
use praxis_protocol::fs::FsReadFileResponse;
use praxis_protocol::fs::FsRemoveParams;
use praxis_protocol::fs::FsRemoveResponse;
use praxis_protocol::fs::FsWriteFileParams;
use praxis_protocol::fs::FsWriteFileResponse;
use praxis_protocol::jsonrpc_lite::JSONRPCErrorError;

use crate::CopyOptions;
use crate::CreateDirectoryOptions;
use crate::ExecutorFileSystem;
use crate::RemoveOptions;

const INVALID_REQUEST_ERROR_CODE: i64 = -32600;
const INTERNAL_ERROR_CODE: i64 = -32603;

#[derive(Clone)]
pub struct FsJsonRpcHandler {
    file_system: Arc<dyn ExecutorFileSystem>,
}

impl FsJsonRpcHandler {
    pub fn new(file_system: Arc<dyn ExecutorFileSystem>) -> Self {
        Self { file_system }
    }

    pub async fn read_file(
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

    pub async fn write_file(
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

    pub async fn create_directory(
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

    pub async fn get_metadata(
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

    pub async fn read_directory(
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

    pub async fn remove(
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

    pub async fn copy(&self, params: FsCopyParams) -> Result<FsCopyResponse, JSONRPCErrorError> {
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

fn invalid_request(message: impl Into<String>) -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: INVALID_REQUEST_ERROR_CODE,
        message: message.into(),
        data: None,
    }
}

fn internal_error(message: impl Into<String>) -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: INTERNAL_ERROR_CODE,
        message: message.into(),
        data: None,
    }
}

fn map_fs_error(err: io::Error) -> JSONRPCErrorError {
    if err.kind() == io::ErrorKind::InvalidInput {
        invalid_request(err.to_string())
    } else {
        internal_error(err.to_string())
    }
}
