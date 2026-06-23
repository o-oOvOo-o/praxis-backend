use crate::ClientNotification;
use crate::ClientRequest;
use crate::GatewayCapability;
use crate::GatewayCapabilityKind;
use crate::GatewayClientInfo;
use crate::GatewayErrorPayload;
use crate::GatewayEventEnvelope;
use crate::GatewayMetadata;
use crate::GatewayMode;
use crate::GatewayRequestEnvelope;
use crate::GatewayResponseEnvelope;
use crate::GatewayTransport;
use crate::HostExtensionInfo;
use crate::HostKind;
use crate::MetraBridgeDescriptor;
use crate::MetraSemanticSnapshot;
use crate::MetraSurfaceDescriptor;
use crate::ServerNotification;
use crate::ServerRequest;
use crate::experimental_api::experimental_fields;
use crate::export_client_notification_schemas;
use crate::export_client_param_schemas;
use crate::export_client_response_schemas;
use crate::export_client_responses;
use crate::export_server_notification_schemas;
use crate::export_server_param_schemas;
use crate::export_server_response_schemas;
use crate::export_server_responses;
use crate::protocol::common::EXPERIMENTAL_CLIENT_METHOD_PARAM_TYPES;
use crate::protocol::common::EXPERIMENTAL_CLIENT_METHOD_RESPONSE_TYPES;
use crate::protocol::common::EXPERIMENTAL_CLIENT_METHODS;
use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use praxis_protocol::protocol::RolloutLine;
use schemars::JsonSchema;
use schemars::schema_for;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use ts_rs::TS;

pub(crate) const GENERATED_TS_HEADER: &str = "// GENERATED CODE! DO NOT MODIFY BY HAND!\n\n";
const IGNORED_DEFINITIONS: &[&str] = &["Option<()>"];
const SPECIAL_DEFINITIONS: &[&str] = &[
    "ClientNotification",
    "ClientRequest",
    "ServerNotification",
    "ServerRequest",
];
const EXCLUDED_SERVER_NOTIFICATION_METHODS_FOR_JSON: &[&str] = &["rawResponseItem/completed"];

#[derive(Clone)]
pub struct GeneratedSchema {
    namespace: Option<String>,
    logical_name: String,
    value: Value,
}

impl GeneratedSchema {
    fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    fn logical_name(&self) -> &str {
        &self.logical_name
    }

    fn value(&self) -> &Value {
        &self.value
    }
}

type JsonSchemaEmitter = fn(&Path) -> Result<GeneratedSchema>;
pub fn generate_types(out_dir: &Path, prettier: Option<&Path>) -> Result<()> {
    generate_ts(out_dir, prettier)?;
    generate_json(out_dir)?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub struct GenerateTsOptions {
    pub generate_indices: bool,
    pub ensure_headers: bool,
    pub run_prettier: bool,
    pub experimental_api: bool,
}

impl Default for GenerateTsOptions {
    fn default() -> Self {
        Self {
            generate_indices: true,
            ensure_headers: true,
            run_prettier: true,
            experimental_api: false,
        }
    }
}

pub fn generate_ts(out_dir: &Path, prettier: Option<&Path>) -> Result<()> {
    generate_ts_with_options(out_dir, prettier, GenerateTsOptions::default())
}

pub fn generate_ts_with_options(
    out_dir: &Path,
    prettier: Option<&Path>,
    options: GenerateTsOptions,
) -> Result<()> {
    ensure_dir(out_dir)?;

    ClientRequest::export_all_to(out_dir)?;
    export_client_responses(out_dir)?;
    ClientNotification::export_all_to(out_dir)?;

    ServerRequest::export_all_to(out_dir)?;
    export_server_responses(out_dir)?;
    ServerNotification::export_all_to(out_dir)?;

    GatewayCapability::export_all_to(out_dir)?;
    GatewayCapabilityKind::export_all_to(out_dir)?;
    GatewayClientInfo::export_all_to(out_dir)?;
    GatewayErrorPayload::export_all_to(out_dir)?;
    GatewayEventEnvelope::export_all_to(out_dir)?;
    GatewayMetadata::export_all_to(out_dir)?;
    GatewayMode::export_all_to(out_dir)?;
    GatewayRequestEnvelope::export_all_to(out_dir)?;
    GatewayResponseEnvelope::export_all_to(out_dir)?;
    GatewayTransport::export_all_to(out_dir)?;
    HostExtensionInfo::export_all_to(out_dir)?;
    HostKind::export_all_to(out_dir)?;
    MetraBridgeDescriptor::export_all_to(out_dir)?;
    MetraSemanticSnapshot::export_all_to(out_dir)?;
    MetraSurfaceDescriptor::export_all_to(out_dir)?;

    if !options.experimental_api {
        filter_experimental_ts(out_dir)?;
    }

    if options.generate_indices {
        generate_index_ts(out_dir)?;
    }

    // Ensure our header is present on all TS files.
    let mut ts_files = Vec::new();
    let should_collect_ts_files =
        options.ensure_headers || (options.run_prettier && prettier.is_some());
    if should_collect_ts_files {
        ts_files = ts_files_in_recursive(out_dir)?;
    }

    if options.ensure_headers {
        let worker_count = thread::available_parallelism()
            .map_or(1, usize::from)
            .min(ts_files.len().max(1));
        let chunk_size = ts_files.len().div_ceil(worker_count);
        thread::scope(|scope| -> Result<()> {
            let mut workers = Vec::new();
            for chunk in ts_files.chunks(chunk_size.max(1)) {
                workers.push(scope.spawn(move || -> Result<()> {
                    for file in chunk {
                        prepend_header_if_missing(file)?;
                    }
                    Ok(())
                }));
            }

            for worker in workers {
                worker
                    .join()
                    .map_err(|_| anyhow!("TypeScript header worker panicked"))??;
            }

            Ok(())
        })?;
    }

    // Optionally run Prettier on all generated TS files.
    if options.run_prettier
        && let Some(prettier_bin) = prettier
        && !ts_files.is_empty()
    {
        let status = Command::new(prettier_bin)
            .arg("--write")
            .arg("--log-level")
            .arg("warn")
            .args(ts_files.iter().map(|p| p.as_os_str()))
            .status()
            .with_context(|| format!("Failed to invoke Prettier at {}", prettier_bin.display()))?;
        if !status.success() {
            return Err(anyhow!("Prettier failed with status {status}"));
        }
    }

    Ok(())
}

pub fn generate_json(out_dir: &Path) -> Result<()> {
    generate_json_with_experimental(out_dir, /*experimental_api*/ false)
}

pub fn generate_internal_json_schema(out_dir: &Path) -> Result<()> {
    ensure_dir(out_dir)?;
    write_json_schema::<RolloutLine>(out_dir, "RolloutLine")?;
    Ok(())
}

pub fn generate_json_with_experimental(out_dir: &Path, experimental_api: bool) -> Result<()> {
    ensure_dir(out_dir)?;
    let envelope_emitters: Vec<JsonSchemaEmitter> = vec![
        |d| write_json_schema_with_return::<crate::RequestId>(d, "RequestId"),
        |d| write_json_schema_with_return::<crate::JSONRPCMessage>(d, "JSONRPCMessage"),
        |d| write_json_schema_with_return::<crate::JSONRPCRequest>(d, "JSONRPCRequest"),
        |d| write_json_schema_with_return::<crate::JSONRPCNotification>(d, "JSONRPCNotification"),
        |d| write_json_schema_with_return::<crate::JSONRPCResponse>(d, "JSONRPCResponse"),
        |d| write_json_schema_with_return::<crate::JSONRPCError>(d, "JSONRPCError"),
        |d| write_json_schema_with_return::<crate::JSONRPCErrorError>(d, "JSONRPCErrorError"),
        |d| write_json_schema_with_return::<crate::ClientRequest>(d, "ClientRequest"),
        |d| write_json_schema_with_return::<crate::ServerRequest>(d, "ServerRequest"),
        |d| write_json_schema_with_return::<crate::ClientNotification>(d, "ClientNotification"),
        |d| write_json_schema_with_return::<crate::ServerNotification>(d, "ServerNotification"),
    ];

    let mut schemas: Vec<GeneratedSchema> = Vec::new();
    for emit in &envelope_emitters {
        schemas.push(emit(out_dir)?);
    }

    schemas.extend(export_client_param_schemas(out_dir)?);
    schemas.extend(export_client_response_schemas(out_dir)?);
    schemas.extend(export_server_param_schemas(out_dir)?);
    schemas.extend(export_server_response_schemas(out_dir)?);
    schemas.extend(export_client_notification_schemas(out_dir)?);
    schemas.extend(export_server_notification_schemas(out_dir)?);

    let mut bundle = build_schema_bundle(schemas)?;
    if !experimental_api {
        filter_experimental_schema(&mut bundle)?;
    }
    write_pretty_json(
        out_dir.join("praxis_app_gateway_protocol.schemas.json"),
        &bundle,
    )?;

    if !experimental_api {
        filter_experimental_json_files(out_dir)?;
    }

    Ok(())
}

mod experimental_filter;
mod generated_files;
mod json_schema;

pub(crate) use self::experimental_filter::filter_experimental_ts_tree;
use self::experimental_filter::{
    filter_experimental_json_files, filter_experimental_schema, filter_experimental_ts,
};
pub(crate) use self::generated_files::generate_index_ts_tree;
use self::generated_files::{generate_index_ts, prepend_header_if_missing, ts_files_in_recursive};
pub(crate) use self::json_schema::write_json_schema;
use self::json_schema::{
    build_schema_bundle, ensure_dir, write_json_schema_with_return, write_pretty_json,
};

#[cfg(test)]
mod tests;
