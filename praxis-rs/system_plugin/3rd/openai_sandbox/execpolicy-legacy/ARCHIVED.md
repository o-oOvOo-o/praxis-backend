# Archived legacy exec policy matcher

`praxis-execpolicy-legacy` is intentionally no longer a workspace member.
Praxis runtime policy now lives in `praxis-execpolicy` plus AgentOS intent,
capability, and resource-lease gates.

The old matcher is kept only as source archaeology while migration fixtures are
being retired. Do not add new production dependencies on this crate.
