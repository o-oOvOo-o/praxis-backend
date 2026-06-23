use super::*;

/// EXPERIMENTAL - start a thread-scoped realtime session.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeStartParams {
    pub thread_id: String,
    pub prompt: String,
    #[ts(optional = nullable)]
    pub session_id: Option<String>,
}

/// EXPERIMENTAL - response for starting thread realtime.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeStartResponse {}

/// EXPERIMENTAL - append audio input to thread realtime.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeAppendAudioParams {
    pub thread_id: String,
    pub audio: ThreadRealtimeAudioChunk,
}

/// EXPERIMENTAL - response for appending realtime audio input.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeAppendAudioResponse {}

/// EXPERIMENTAL - raw audio input for standalone transcription.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AudioTranscribeAudio {
    pub data: String,
    pub media_type: String,
}

/// EXPERIMENTAL - transcription submit behavior requested by backend config.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum AudioTranscriptionSubmitMode {
    #[default]
    InsertIntoComposer,
    AutoSubmit,
}

/// EXPERIMENTAL - transcribe audio without starting a realtime turn.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AudioTranscribeParams {
    #[ts(optional = nullable)]
    pub provider_id: Option<String>,
    #[ts(optional = nullable)]
    pub model: Option<String>,
    pub audio: AudioTranscribeAudio,
    #[ts(optional = nullable)]
    pub language: Option<String>,
}

/// EXPERIMENTAL - response for standalone audio transcription.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AudioTranscribeResponse {
    pub text: String,
    pub provider_id: String,
    #[ts(optional = nullable)]
    pub model: Option<String>,
    pub submit_mode: AudioTranscriptionSubmitMode,
}

/// EXPERIMENTAL - append text input to thread realtime.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeAppendTextParams {
    pub thread_id: String,
    pub text: String,
}

/// EXPERIMENTAL - response for appending realtime text input.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeAppendTextResponse {}

/// EXPERIMENTAL - stop thread realtime.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeStopParams {
    pub thread_id: String,
}

/// EXPERIMENTAL - response for stopping thread realtime.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeStopResponse {}

/// EXPERIMENTAL - emitted when thread realtime startup is accepted.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeStartedNotification {
    pub thread_id: String,
    pub session_id: Option<String>,
    pub version: RealtimeConversationVersion,
}

/// EXPERIMENTAL - raw non-audio thread realtime item emitted by the backend.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeItemAddedNotification {
    pub thread_id: String,
    pub item: JsonValue,
}

/// EXPERIMENTAL - flat transcript delta emitted whenever realtime
/// transcript text changes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeTranscriptUpdatedNotification {
    pub thread_id: String,
    pub role: String,
    pub text: String,
}

/// EXPERIMENTAL - streamed output audio emitted by thread realtime.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeOutputAudioDeltaNotification {
    pub thread_id: String,
    pub audio: ThreadRealtimeAudioChunk,
}

/// EXPERIMENTAL - emitted when thread realtime encounters an error.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeErrorNotification {
    pub thread_id: String,
    pub message: String,
}

/// EXPERIMENTAL - emitted when thread realtime transport closes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeClosedNotification {
    pub thread_id: String,
    pub reason: Option<String>,
}
