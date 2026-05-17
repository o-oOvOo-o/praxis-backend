use std::collections::BTreeMap;

use crate::approvals::ElicitationAction;
use crate::approvals::ElicitationRequest;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use ts_rs::TS;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum McpServerElicitationAction {
    Accept,
    Decline,
    Cancel,
}

impl McpServerElicitationAction {
    pub fn to_core(self) -> ElicitationAction {
        self.into()
    }
}

impl From<McpServerElicitationAction> for ElicitationAction {
    fn from(value: McpServerElicitationAction) -> Self {
        match value {
            McpServerElicitationAction::Accept => Self::Accept,
            McpServerElicitationAction::Decline => Self::Decline,
            McpServerElicitationAction::Cancel => Self::Cancel,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpServerElicitationRequestParams {
    pub thread_id: String,
    /// Active Praxis turn when this elicitation was observed, if app-gateway could correlate one.
    pub turn_id: Option<String>,
    pub server_name: String,
    #[serde(flatten)]
    pub request: McpServerElicitationRequest,
}

/// Typed form schema for MCP `elicitation/create` requests.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpElicitationSchema {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    #[ts(optional, rename = "$schema")]
    pub schema_uri: Option<String>,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub type_: McpElicitationObjectType,
    pub properties: BTreeMap<String, McpElicitationPrimitiveSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub required: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum McpElicitationObjectType {
    Object,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(untagged)]
pub enum McpElicitationPrimitiveSchema {
    Enum(McpElicitationEnumSchema),
    String(McpElicitationStringSchema),
    Number(McpElicitationNumberSchema),
    Boolean(McpElicitationBooleanSchema),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpElicitationStringSchema {
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub type_: McpElicitationStringType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub format: Option<McpElicitationStringFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum McpElicitationStringType {
    String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename_all = "kebab-case")]
pub enum McpElicitationStringFormat {
    Email,
    Uri,
    Date,
    DateTime,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpElicitationNumberSchema {
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub type_: McpElicitationNumberType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub maximum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum McpElicitationNumberType {
    Number,
    Integer,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpElicitationBooleanSchema {
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub type_: McpElicitationBooleanType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum McpElicitationBooleanType {
    Boolean,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(untagged)]
pub enum McpElicitationEnumSchema {
    SingleSelect(McpElicitationSingleSelectEnumSchema),
    MultiSelect(McpElicitationMultiSelectEnumSchema),
    Legacy(McpElicitationLegacyTitledEnumSchema),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpElicitationLegacyTitledEnumSchema {
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub type_: McpElicitationStringType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(rename = "enum")]
    #[ts(rename = "enum")]
    pub enum_: Vec<String>,
    #[serde(rename = "enumNames", skip_serializing_if = "Option::is_none")]
    #[ts(optional, rename = "enumNames")]
    pub enum_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(untagged)]
pub enum McpElicitationSingleSelectEnumSchema {
    Untitled(McpElicitationUntitledSingleSelectEnumSchema),
    Titled(McpElicitationTitledSingleSelectEnumSchema),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpElicitationUntitledSingleSelectEnumSchema {
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub type_: McpElicitationStringType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(rename = "enum")]
    #[ts(rename = "enum")]
    pub enum_: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpElicitationTitledSingleSelectEnumSchema {
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub type_: McpElicitationStringType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(rename = "oneOf")]
    #[ts(rename = "oneOf")]
    pub one_of: Vec<McpElicitationConstOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(untagged)]
pub enum McpElicitationMultiSelectEnumSchema {
    Untitled(McpElicitationUntitledMultiSelectEnumSchema),
    Titled(McpElicitationTitledMultiSelectEnumSchema),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpElicitationUntitledMultiSelectEnumSchema {
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub type_: McpElicitationArrayType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min_items: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_items: Option<u64>,
    pub items: McpElicitationUntitledEnumItems,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpElicitationTitledMultiSelectEnumSchema {
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub type_: McpElicitationArrayType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min_items: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_items: Option<u64>,
    pub items: McpElicitationTitledEnumItems,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum McpElicitationArrayType {
    Array,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct McpElicitationUntitledEnumItems {
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub type_: McpElicitationStringType,
    #[serde(rename = "enum")]
    #[ts(rename = "enum")]
    pub enum_: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct McpElicitationTitledEnumItems {
    #[serde(rename = "anyOf", alias = "oneOf")]
    #[ts(rename = "anyOf")]
    pub any_of: Vec<McpElicitationConstOption>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct McpElicitationConstOption {
    #[serde(rename = "const")]
    #[ts(rename = "const")]
    pub const_: String,
    pub title: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(tag = "mode", rename_all = "camelCase")]
#[ts(tag = "mode")]
pub enum McpServerElicitationRequest {
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    Form {
        #[serde(rename = "_meta")]
        #[ts(rename = "_meta")]
        meta: Option<JsonValue>,
        message: String,
        requested_schema: McpElicitationSchema,
    },
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    Url {
        #[serde(rename = "_meta")]
        #[ts(rename = "_meta")]
        meta: Option<JsonValue>,
        message: String,
        url: String,
        elicitation_id: String,
    },
}

impl TryFrom<ElicitationRequest> for McpServerElicitationRequest {
    type Error = serde_json::Error;

    fn try_from(value: ElicitationRequest) -> Result<Self, Self::Error> {
        match value {
            ElicitationRequest::Form {
                meta,
                message,
                requested_schema,
            } => Ok(Self::Form {
                meta,
                message,
                requested_schema: serde_json::from_value(requested_schema)?,
            }),
            ElicitationRequest::Url {
                meta,
                message,
                url,
                elicitation_id,
            } => Ok(Self::Url {
                meta,
                message,
                url,
                elicitation_id,
            }),
        }
    }
}
