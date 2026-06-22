use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use praxis_app_gateway_protocol::AudioTranscribeParams;
use praxis_app_gateway_protocol::AudioTranscribeResponse;
use praxis_app_gateway_protocol::AudioTranscriptionSubmitMode;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_core::config::TranscriptionSubmitMode;
use praxis_core::transcription::TranscriptionAudio;
use praxis_core::transcription::TranscriptionRequest;
use praxis_core::transcription::TranscriptionRuntime;

use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_PARAMS_ERROR_CODE;
use crate::outgoing_message::ConnectionRequestId;
use crate::praxis_message_processor::PraxisMessageProcessor;

impl PraxisMessageProcessor {
    pub(super) async fn audio_transcribe(
        &mut self,
        request_id: ConnectionRequestId,
        params: AudioTranscribeParams,
    ) {
        let audio_bytes = match decode_audio_data(&params.audio.data) {
            Ok(bytes) => bytes,
            Err(message) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_PARAMS_ERROR_CODE,
                            message,
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };
        let runtime = TranscriptionRuntime::new(self.config.transcription.clone());
        let request = TranscriptionRequest {
            provider_id: params.provider_id,
            model: params.model,
            audio: TranscriptionAudio {
                media_type: params.audio.media_type,
                bytes: audio_bytes,
            },
            language: params.language,
        };

        match runtime.transcribe(request).await {
            Ok(response) => {
                self.outgoing
                    .send_response(
                        request_id,
                        AudioTranscribeResponse {
                            text: response.text,
                            provider_id: response.provider_id,
                            model: response.model,
                            submit_mode: match response.submit_mode {
                                TranscriptionSubmitMode::InsertIntoComposer => {
                                    AudioTranscriptionSubmitMode::InsertIntoComposer
                                }
                                TranscriptionSubmitMode::AutoSubmit => {
                                    AudioTranscriptionSubmitMode::AutoSubmit
                                }
                            },
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: err.to_string(),
                            data: None,
                        },
                    )
                    .await;
            }
        }
    }
}

fn decode_audio_data(data: &str) -> Result<Vec<u8>, String> {
    let payload = data
        .split_once(',')
        .filter(|(prefix, _)| prefix.trim_start().starts_with("data:"))
        .map(|(_, payload)| payload)
        .unwrap_or(data);
    STANDARD
        .decode(payload.trim())
        .map_err(|err| format!("audio.data must be base64 encoded: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_raw_base64_audio() {
        assert_eq!(decode_audio_data("UklGRg==").unwrap(), b"RIFF");
    }

    #[test]
    fn decodes_data_uri_audio() {
        assert_eq!(
            decode_audio_data("data:audio/wav;base64,UklGRg==").unwrap(),
            b"RIFF"
        );
    }
}
