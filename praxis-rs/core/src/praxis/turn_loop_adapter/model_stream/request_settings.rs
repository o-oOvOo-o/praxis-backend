use praxis_loop::outcome::LoopResult;
use praxis_loop::outcome::TurnError;
use praxis_loop::outcome::TurnErrorKind;
use praxis_loop::services::RoundSettings;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;

#[derive(Clone, Debug)]
pub(super) struct PraxisRoundSettings {
    pub(super) model_slug: String,
    pub(super) reasoning: Option<ReasoningEffortConfig>,
    pub(super) service_tier: Option<ServiceTier>,
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

pub(super) fn parse_round_settings(settings: &RoundSettings) -> LoopResult<PraxisRoundSettings> {
    Ok(PraxisRoundSettings {
        model_slug: settings.model.slug.clone(),
        reasoning: parse_request_reasoning(settings.reasoning.as_deref())?.into_option(),
        service_tier: parse_request_service_tier(settings.service_tier.as_deref())?.into_option(),
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
