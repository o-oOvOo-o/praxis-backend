# Codex Compatibility Cleanroom

Praxis owns the product identity, runtime model, App Gateway, TUI, plugin system,
and local state. Codex names may only remain when they describe an external
source, a legacy wire contract, a model slug, or a backward-compatibility alias.

## Classification

### Removed identity residue

- Public install and run instructions should say Praxis.
- README, contribution, CLA, open-source-fund, install, config, sandbox, skills,
  exec, and TUI docs should not point users at the upstream Codex product.
- Packaging metadata should describe Praxis as the shipped program.
- Internal JS REPL helper docs should teach `praxis.*` first.

### Compatibility surface

Keep these behind an explicit Codex compatibility boundary:

- Legacy local state read-through from upstream Codex homes.
- Legacy authentication/config import paths.
- Legacy environment variables such as `CODEX_*`, with Praxis aliases added
  before any deprecation.
- Legacy JS REPL global `codex.*`, implemented as an alias to `praxis.*`.
- Legacy slash commands or import flows that explicitly resume or migrate
  upstream Codex sessions.
- Legacy packaging, binary, or Bazel target aliases needed by existing scripts.
- Wire protocol fields whose public contract is already named Codex.

### External facts

Do not rename these unless the upstream artifact changes:

- Model identifiers that include `codex`, for example `gpt-*-codex`.
- Upstream artifact URLs, checksums, and third-party release locations.
- Imported external session source names when the source is literally Codex.

## Target Module Boundary

Create a narrow `praxis-codex-compat` boundary instead of letting Codex names
spread through core. It can begin as modules inside existing crates and later
be extracted into a crate if dependency direction stays clean.

Suggested ownership:

- `compat::codex::home`: detects and reads legacy Codex home/config/auth paths.
- `compat::codex::env`: maps `CODEX_*` variables to Praxis runtime settings.
- `compat::codex::wire`: owns old wire names, payload aliases, and API path
  aliases that must remain stable.
- `compat::codex::js_repl`: exposes `codex.*` as a thin alias over `praxis.*`.
- `compat::codex::packaging`: keeps legacy CLI/npm/Bazel target aliases.
- `external_agent_migration::sessions::codex`: stays as the importer for
  actual upstream Codex session data.

Everything outside this boundary should use Praxis, provider-neutral, or
domain-neutral names.

## Current Hotspots

The broad scan currently finds thousands of Codex matches because historical
rollout fixtures, tests, model slugs, and external-source samples repeat the
string many times. The actionable hotspots are narrower:

- `praxis_login::OpenAiAccountAuth` is the internal auth name. `CodexAuth`
  should remain only as a compatibility alias where older callers still import
  it.
- `praxis_utils_home_dir::*codex*` and `config_loader`: should own legacy home
  read-through behind `compat::codex::home`.
- `/backend-api/codex/*`, `/codex/safety/arc`, and usage settings URLs: wire
  names for upstream services, owned by `compat::codex::wire`.
- `CODEX_*` environment variables: map through `compat::codex::env` and add
  `PRAXIS_*` aliases before removing legacy names.
- `llm::profiles::openai_responses` is the internal profile module. The
  serialized `codex/responses` profile id remains a compatibility wire value,
  not the internal module name.
- `external_agent_migration::sessions::codex`: keep as the importer for real
  Codex session archives.
- `justfile` and Bazel `//praxis-rs/cli:codex`: keep as build aliases until a
  `praxis` target exists and is exercised by CI/scripts.

## Current Retained Codex Surfaces

Retain these unless a replacement protocol or migration has already shipped:

- Hosted service routes: `/api/codex/*`, `/codex/analytics-events/events`,
  `/codex/tasks/*`, `/codex/safety/arc`, and `/codex/device`.
- Model slugs and model migration text containing `gpt-*-codex`.
- Rate-limit and telemetry wire buckets whose canonical id is `codex`.
- `CODEX_*` environment variables used as legacy read-through inputs, with
  Praxis aliases preferred internally.
- `.codex` filesystem paths when they mean upstream Codex home, project config
  compatibility, external session import, or sandbox protected subdirectories.
- `/codex` slash command and `SessionLookupSource::Codex`, because the source is
  literally upstream Codex sessions.
- `external_agent_migration::sessions::codex`, because it is the
  anti-corruption layer for real Codex rollout/session formats.
- MCP or app-gateway wire aliases such as `codex/event` and
  `codex/sandbox-state`, when external clients already speak those names.

## Recently Cleaned Identity Residue

- Issue template product wording now points at Praxis CLI and `@openai/praxis`.
- Test support module names use `test_praxis` / `TestPraxis`.
- `TestPraxis` exposes its live thread handle as `thread`, not `codex`.
- The exec policy example extension moved from `.codexpolicy` to
  `.praxispolicy`.
- The local Responses WebSocket helper script now documents `praxis --profile`
  and the `test_praxis` flow.

## Brooks-Style Design Rules

- Conceptual integrity: a core type should not be named Codex unless its domain
  is specifically upstream Codex compatibility.
- Separation of concerns: compatibility translates at the edges; product logic
  consumes Praxis or neutral abstractions.
- Information hiding: callers should not know whether a config value came from
  Praxis state or a legacy Codex fallback.
- Change locality: removing a legacy Codex alias should touch the compatibility
  module and tests, not App Gateway, TUI, core session logic, and packaging at
  once.
- Build stability: introduce Praxis aliases before deleting Codex aliases from
  scripts, CI, and Bazel targets.

## Migration Plan

1. Public identity cleanup: remove user-facing Codex product branding from docs,
   package metadata, and primary commands.
2. Add additive Praxis aliases for environment variables, JS globals, binary
   targets, and config names while preserving Codex aliases.
3. Move direct Codex constants into `compat::codex::*` modules with tests around
   fallback order and alias behavior.
4. Update internal callers to consume Praxis or neutral abstractions.
5. After downstream scripts are migrated, remove legacy aliases in one scoped
   compatibility change.

## Non-Breaking Constraints

- Do not change model slugs.
- Do not delete legacy auth/config read-through until Praxis aliases have shipped
  and migration is verified.
- Do not rename public wire fields without compatibility serializers.
- Do not delete build targets or command aliases until replacement targets are
  available and exercised by local scripts.
