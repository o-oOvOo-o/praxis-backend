use praxis_network_proxy::normalize_host;
use praxis_protocol::approvals::NetworkPolicyAmendment;
use praxis_protocol::approvals::NetworkPolicyRuleAction;
use praxis_protocol::protocol::NetworkApprovalContext;

use crate::network_policy_decision::execpolicy_network_rule_amendment;
use crate::praxis::Session;

impl Session {
    pub(crate) async fn persist_network_policy_amendment(
        &self,
        amendment: &NetworkPolicyAmendment,
        network_approval_context: &NetworkApprovalContext,
    ) -> anyhow::Result<()> {
        let host =
            Self::validated_network_policy_amendment_host(amendment, network_approval_context)?;
        let praxis_home = self
            .state
            .lock()
            .await
            .session_configuration
            .praxis_home()
            .clone();
        let execpolicy_amendment =
            execpolicy_network_rule_amendment(amendment, network_approval_context, &host);

        self.update_runtime_network_policy(amendment, &host).await?;

        self.services
            .exec_policy
            .append_network_rule_and_update(
                &praxis_home,
                &host,
                execpolicy_amendment.protocol,
                execpolicy_amendment.decision,
                Some(execpolicy_amendment.justification),
            )
            .await
            .map_err(|err| {
                anyhow::anyhow!("failed to persist network policy amendment to execpolicy: {err}")
            })?;

        Ok(())
    }

    pub(crate) fn validated_network_policy_amendment_host(
        amendment: &NetworkPolicyAmendment,
        network_approval_context: &NetworkApprovalContext,
    ) -> anyhow::Result<String> {
        let approved_host = normalize_host(&network_approval_context.host);
        let amendment_host = normalize_host(&amendment.host);
        if amendment_host != approved_host {
            return Err(anyhow::anyhow!(
                "network policy amendment host '{}' does not match approved host '{}'",
                amendment.host,
                network_approval_context.host
            ));
        }
        Ok(approved_host)
    }

    async fn update_runtime_network_policy(
        &self,
        amendment: &NetworkPolicyAmendment,
        host: &str,
    ) -> anyhow::Result<()> {
        let Some(started_network_proxy) = self.services.network_proxy.as_ref() else {
            return Ok(());
        };

        let proxy = started_network_proxy.proxy();
        match amendment.action {
            NetworkPolicyRuleAction::Allow => proxy
                .add_allowed_domain(host)
                .await
                .map_err(|err| anyhow::anyhow!("failed to update runtime allowlist: {err}"))?,
            NetworkPolicyRuleAction::Deny => proxy
                .add_denied_domain(host)
                .await
                .map_err(|err| anyhow::anyhow!("failed to update runtime denylist: {err}"))?,
        }
        Ok(())
    }
}
