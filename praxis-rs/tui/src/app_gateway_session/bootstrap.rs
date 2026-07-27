use super::*;

impl AppGatewaySession {
    pub(crate) async fn read_account(&mut self) -> Result<GetAccountResponse> {
        let account_request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::GetAccount {
                request_id: account_request_id,
                params: GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
            .wrap_err("account/read failed")
    }

    pub(crate) async fn bootstrap(&mut self, config: &Config) -> Result<AppGatewayBootstrap> {
        let account = self
            .read_account()
            .await
            .wrap_err("account/read failed during TUI bootstrap")?;
        let model_request_id = self.next_request_id();
        let models: ModelListResponse = self
            .client
            .request_typed(ClientRequest::ModelList {
                request_id: model_request_id,
                params: ModelListParams {
                    cursor: None,
                    limit: None,
                    include_hidden: Some(true),
                },
            })
            .await
            .wrap_err("model/list failed during TUI bootstrap")?;
        let default_model = resolve_model_id(
            config.model.as_deref(),
            Some(config.model_provider_id.as_str()),
            &models.data,
        )
        .wrap_err("model/list returned no models for TUI bootstrap")?;
        let available_models = models
            .data
            .into_iter()
            .map(model_preset_from_api_model)
            .collect::<Vec<_>>();

        let (
            account_auth_mode,
            account_email,
            auth_mode,
            status_account_display,
            plan_type,
            feedback_audience,
            has_chatgpt_account,
        ) = match account.account {
            Some(Account::ApiKey {}) => (
                Some(AuthMode::ApiKey),
                None,
                Some(TelemetryAuthMode::ApiKey),
                Some(StatusAccountDisplay::ApiKey),
                None,
                FeedbackAudience::External,
                false,
            ),
            Some(Account::Chatgpt { email, plan_type }) => {
                let feedback_audience = feedback_audience_from_account_email(Some(&email));
                (
                    Some(AuthMode::Chatgpt),
                    Some(email.clone()),
                    Some(TelemetryAuthMode::Chatgpt),
                    Some(StatusAccountDisplay::ChatGpt {
                        email: Some(email),
                        plan: Some(plan_type_display_name(plan_type)),
                    }),
                    Some(plan_type),
                    feedback_audience,
                    true,
                )
            }
            None => (
                None,
                None,
                None,
                None,
                None,
                FeedbackAudience::External,
                false,
            ),
        };
        Ok(AppGatewayBootstrap {
            account_auth_mode,
            account_email,
            auth_mode,
            status_account_display,
            plan_type,
            default_model,
            feedback_audience,
            has_chatgpt_account,
            available_models,
        })
    }
}
