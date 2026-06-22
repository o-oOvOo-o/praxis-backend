mod builder;
mod input;

use crate::state::SessionServices;

pub(super) use input::ServicesBootstrapInput;

pub(super) async fn build(input: ServicesBootstrapInput) -> anyhow::Result<SessionServices> {
    builder::build(input).await
}
