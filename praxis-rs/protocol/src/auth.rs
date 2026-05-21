use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use strum_macros::Display;
use ts_rs::TS;

/// Authentication mode for OpenAI-backed providers.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Display, JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    /// OpenAI API key provided by the caller and stored by Praxis.
    ApiKey,
    /// ChatGPT OAuth managed by Praxis.
    Chatgpt,
    /// ChatGPT auth tokens supplied by an external host app.
    #[serde(rename = "chatgptAuthTokens")]
    #[ts(rename = "chatgptAuthTokens")]
    #[strum(serialize = "chatgptAuthTokens")]
    ChatgptAuthTokens,
}
