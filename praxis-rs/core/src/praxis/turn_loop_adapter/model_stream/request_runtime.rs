//! Provider request settings and turn-context projection.

#![allow(unused_imports)]

use super::*;

pub(in crate::praxis::turn_loop_adapter::model_stream) mod request_context {
    use std::sync::Arc;

    use praxis_loop::outcome::LoopResult;
    use praxis_loop::services::ModelRequest;

    use super::super::super::Session;
    use super::super::super::TurnContext;
    use super::request_context_update;
    use super::request_settings;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn resolve_request_turn_context(
        session: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        request: &ModelRequest,
    ) -> LoopResult<Arc<TurnContext>> {
        let settings = request_settings::parse_round_settings(&request.settings)?;
        if !request_context_update::round_settings_change_context(turn_context, &settings) {
            return Ok(Arc::clone(turn_context));
        }

        Ok(Arc::new(
            request_context_update::apply_round_settings(session, turn_context, settings).await,
        ))
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod request_context_update {
    use std::sync::Arc;

    use super::super::super::Session;
    use super::super::super::TurnContext;
    use super::request_settings::PraxisRoundSettings;

    pub(in crate::praxis::turn_loop_adapter::model_stream) fn round_settings_change_context(
        turn_context: &TurnContext,
        settings: &PraxisRoundSettings,
    ) -> bool {
        settings.model_slug != turn_context.model_info.slug
            || settings.reasoning.is_some() && settings.reasoning != turn_context.reasoning_effort
            || settings.service_tier.is_some()
                && settings.service_tier != turn_context.config.service_tier
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn apply_round_settings(
        session: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        settings: PraxisRoundSettings,
    ) -> TurnContext {
        let mut effective_context = turn_context
            .with_model(settings.model_slug, &session.services.models_manager)
            .await;
        let mut effective_config = (*effective_context.config).clone();
        let effective_reasoning = settings
            .reasoning
            .or_else(|| effective_context.reasoning_effort.clone());
        let effective_service_tier = settings.service_tier.or(effective_config.service_tier);
        effective_config.model_reasoning_effort = effective_reasoning.clone();
        effective_config.service_tier = effective_service_tier;
        effective_context.config = Arc::new(effective_config);
        effective_context.reasoning_effort = effective_reasoning.clone();
        effective_context.collaboration_mode = effective_context.collaboration_mode.with_updates(
            Some(effective_context.model_info.slug.clone()),
            Some(effective_reasoning),
            None,
        );

        effective_context
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod request_settings {
    use praxis_loop::outcome::LoopResult;
    use praxis_loop::outcome::TurnError;
    use praxis_loop::outcome::TurnErrorKind;
    use praxis_loop::services::RoundSettings;
    use praxis_protocol::config_types::ServiceTier;
    use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;

    #[derive(Clone, Debug)]
    pub(in crate::praxis::turn_loop_adapter::model_stream) struct PraxisRoundSettings {
        pub(in crate::praxis::turn_loop_adapter::model_stream) model_slug: String,
        pub(in crate::praxis::turn_loop_adapter::model_stream) reasoning:
            Option<ReasoningEffortConfig>,
        pub(in crate::praxis::turn_loop_adapter::model_stream) service_tier: Option<ServiceTier>,
    }

    enum ParsedSetting<T> {
        Inherit,
        Override(T),
    }

    impl<T> ParsedSetting<T> {
        fn into_option(self) -> Option<T> {
            match self {
                Self::Inherit => None,
                Self::Override(value) => Some(value),
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) fn parse_round_settings(
        settings: &RoundSettings,
    ) -> LoopResult<PraxisRoundSettings> {
        Ok(PraxisRoundSettings {
            model_slug: settings.model.slug.clone(),
            reasoning: parse_request_reasoning(settings.reasoning.as_deref())?.into_option(),
            service_tier: parse_request_service_tier(settings.service_tier.as_deref())?
                .into_option(),
        })
    }

    fn parse_request_reasoning(
        value: Option<&str>,
    ) -> LoopResult<ParsedSetting<ReasoningEffortConfig>> {
        let Some(value) = trimmed_setting(value) else {
            return Ok(ParsedSetting::Inherit);
        };
        value
            .parse::<ReasoningEffortConfig>()
            .map(ParsedSetting::Override)
            .map_err(|err| {
                TurnError::new(
                    TurnErrorKind::Internal,
                    format!("invalid loop reasoning setting `{value}`: {err}"),
                )
            })
    }

    fn parse_request_service_tier(value: Option<&str>) -> LoopResult<ParsedSetting<ServiceTier>> {
        let Some(value) = trimmed_setting(value) else {
            return Ok(ParsedSetting::Inherit);
        };
        match value.to_ascii_lowercase().as_str() {
            "fast" => Ok(ParsedSetting::Override(ServiceTier::Fast)),
            "flex" => Ok(ParsedSetting::Override(ServiceTier::Flex)),
            _ => Err(TurnError::new(
                TurnErrorKind::Internal,
                format!("invalid loop service tier setting `{value}`"),
            )),
        }
    }

    fn trimmed_setting(value: Option<&str>) -> Option<&str> {
        value.map(str::trim).filter(|value| !value.is_empty())
    }
}
