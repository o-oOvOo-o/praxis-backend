use crate::feedback_tags;
use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) fn record_model_request_start(sess: &Session, turn_context: &TurnContext) {
    let permissions = turn_context.effective_permissions();
    feedback_tags!(
        model = turn_context.model_info.slug.clone(),
        approval_policy = permissions.approval_policy.value(),
        sandbox_policy = permissions.sandbox_policy.get(),
        effort = turn_context.reasoning_effort,
        auth_mode = sess.services.auth_manager.auth_mode(),
        features = sess.features.enabled_features(),
    );
}
