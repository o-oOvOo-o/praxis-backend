# Canonical model catalog and reasoning defaults

The backend `models_manager::catalog` owns catalog composition for the active resolved configuration. It merges the current provider catalog, first-party providers, plugin models, native local models, and models declared in the effective configuration profiles. Provider/model pairs are deduplicated without rewriting provider identifiers. An explicit custom catalog remains authoritative for its provider.

The App Gateway projects this catalog into `model/list`. The TUI projects those returned presets into selection metadata; it does not scan home directories, deserialize a second configuration schema, or inject its own models. Picker priorities come from the shared model metadata. Worker tool guidance refers to its generated available-model catalog instead of hardcoding a recommended generation.

`praxis_protocol::openai_models::DEFAULT_REASONING_EFFORT` is `medium`. Picker conversion, resolved model metadata, and model-switch fallback use this product default, regardless of the remote catalog default. Explicit supported effort selections remain intact. Provider wire adapters retain their native semantics: Kimi K3 maps the medium intent to native `high`, while K2.7 enables thinking without an effort field. Models without reasoning support must not acquire unsupported wire parameters.

The TUI model catalog is an in-memory value, so listing it is infallible. There is no lock-error branch or retry message.

Turn-loop adapter preparation, prompt translation, history projection, tool execution, and event emission live in separate modules under `preparation/` and `tools/`; the three agent loops remain unchanged.

Regression coverage lives in protocol `tests/model_catalog_policy.rs`, core `tests/model_catalog_policy.rs`, the turn-context reasoning tests, and the Gateway/TUI model-list and picker tests.

Validation on 2026-09-05: the three protocol policy integration tests passed. Core, Gateway, and TUI production libraries compiled during the focused test builds. Core integration tests were blocked before execution by `core_test_support` referencing the undeclared `praxis_rollout` crate. TUI unit tests were blocked by existing missing test modules and stale test APIs (799 compiler errors), so the existing model-picker snapshots could not be regenerated or reviewed. These results are not a passing full regression run.

The 2026-09-06 cleanup routes sub-agent catalog construction through the same catalog, followed by approval and credential filtering. It preserves provider/model pairs. TUI capability lookup uses the same provider-aware selection predicate as the model picker. Unscoped preset ownership is resolved in the core catalog projection.

Thread view restoration is shared by workspace and agent selection in `tui/src/app/thread_view_restore.rs`; replay-only behavior remains at the callers. Session-source metadata overlays are owned by `SessionSource`, and Skill protocol conversion is owned by `praxis-core-skills`.

Validation: the source-metadata integration test passed. Skill projection testing stopped in dependency compilation because F: ran out of disk space; the subsequent TUI/Gateway library check was cancelled. Cross-provider regression coverage was added but has not executed. No full regression pass is claimed.

The subsequent review fix stores a prepared sub-agent catalog snapshot in ModelsManager. Discovery runs on a blocking worker during async initialization/refresh; synchronous turn tool assembly only reads the snapshot. Changed resolved configuration triggers async preparation before assembly. Explicit custom catalogs exclude all current-provider backfill sources, including active model, profiles, and local discovery. API provider IDs are retained in scoped preset IDs, and remote-only selections carry no fabricated local provider configuration.

Skill and generic multi-select pickers now share filtering, selection preservation, and checkbox labels. The two shared-list tests passed in an isolated rustc harness. TUI library type checking was attempted with two jobs but stopped in dependency metadata generation with disk-full errors before completion; no successful library check is claimed.
