# Codex Compatibility Cleanroom

Praxis owns the product identity, runtime model, App Gateway, TUI, plugin system,
and local state. Codex names may only remain when they describe an external
source, a legacy wire contract, or a model slug.

## Classification

### Removed identity residue

- Public install and run instructions should say Praxis.
- README, contribution, CLA, open-source-fund, install, config, sandbox, skills,
  exec, and TUI docs should not point users at the external Codex product.
- Packaging metadata should describe Praxis as the shipped program.
- Internal JS REPL helper docs should teach `praxis.*` first.

### Compatibility surface

Keep these behind an explicit Codex compatibility boundary:

- Legacy local state read-through from external Codex homes.
- Legacy authentication/config import paths.
- Legacy environment variables such as `CODEX_*`, with Praxis aliases added
  before any deprecation.
- Legacy slash commands or import flows that explicitly resume or migrate
  external Codex sessions.
- Wire protocol fields whose public contract is already named Codex.

### External facts

Do not rename these unless the external artifact changes:

- Model identifiers that include `codex`, for example `gpt-*-codex`.
- External artifact URLs, checksums, and third-party release locations.
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
- `external_agent_migration::sessions::codex`: stays as the importer for
  actual external Codex session data.

Everything outside this boundary should use Praxis, provider-neutral, or
domain-neutral names.

## Current Hotspots

The broad scan currently finds thousands of Codex matches because historical
rollout fixtures, tests, model slugs, and external-source samples repeat the
string many times. The actionable hotspots are narrower:

- `praxis_login::OpenAiAccountAuth` is the only internal OpenAI account auth
  name. Old login aliases are removed; compatibility belongs in explicit
  import/config readers, not generic Rust exports.
- `praxis_utils_home_dir::*codex*` and `config_loader`: should own legacy home
  read-through behind `compat::codex::home`.
- `/backend-api/codex/*`, `/codex/safety/arc`, and usage settings URLs: wire
  names for hosted compatibility services, owned by `compat::codex::wire`.
- `CODEX_*` environment variables: map through `compat::codex::env` and add
  `PRAXIS_*` aliases before removing legacy names.
- `llm::profiles::openai_responses` is the internal profile module. The
  serialized `codex/responses` profile id remains a compatibility wire value,
  not the internal module name.
- `external_agent_migration::sessions::codex`: keep as the importer for real
  Codex session archives.
- Build and release scripts should use the `praxis` binary and package names.

## Current Retained Codex Surfaces

Retain these unless a replacement protocol or migration has already shipped:

- Hosted service routes: `/api/codex/*`, `/codex/analytics-events/events`,
  `/codex/tasks/*`, `/codex/safety/arc`, and `/codex/device`.
- Model slugs and model migration text containing `gpt-*-codex`.
- Rate-limit and telemetry wire buckets whose canonical id is `codex`.
- `CODEX_*` environment variables used as legacy read-through inputs, with
  Praxis aliases preferred internally.
- `.codex` filesystem paths when they mean external Codex home, project config
  compatibility, external session import, or sandbox protected subdirectories.
- `/codex` slash command and `SessionLookupSource::Codex`, because the source is
  literally external Codex sessions.
- `external_agent_migration::sessions::codex`, because it is the
  anti-corruption layer for real Codex rollout/session formats.
- MCP or app-gateway wire aliases such as `codex/event` and
  `codex/sandbox-state`, when external clients already speak those names.

## Recently Cleaned Identity Residue

- Issue template product wording now points at Praxis CLI and `@praxis/praxis`.
- Test support module names use `test_praxis` / `TestPraxis`.
- `TestPraxis` exposes its live thread handle as `thread`, not `codex`.
- The exec policy example extension moved from `.codexpolicy` to
  `.praxispolicy`.
- The local Responses WebSocket helper script now documents `praxis --profile`
  and the `test_praxis` flow.
- App Gateway docs now document `praxisHome` and `praxisErrorInfo`, matching the
  current protocol schema instead of legacy field names.
- Startup and shell environment code now call neutral external-agent state
  scrubbing APIs; raw `CODEX_*` constants live in the home-dir compatibility
  boundary.
- Guardian approval prompts and fixture repositories use Praxis wording and
  Praxis example repositories.
- npm native packaging uses only the Praxis layout (`vendor/<target>/praxis`).
- Release, code-sign, DotSlash, and installer paths now target the `praxis`
  binary instead of stale `codex` build outputs.
- TUI snapshots now use Praxis product wording, and the unused old `oss-story`
  recording was removed instead of carrying stale `codex_event` logs.
- MCP tool calls accept the `praxis` tool name only; the hidden `codex` tool
  alias was removed.

## Brooks-Style Design Rules

- Conceptual integrity: a core type should not be named Codex unless its domain
  is specifically external Codex compatibility.
- Separation of concerns: compatibility translates at the edges; product logic
  consumes Praxis or neutral abstractions.
- Information hiding: callers should not know whether a config value came from
  Praxis state or a legacy Codex fallback.
- Change locality: removing a legacy Codex alias should touch the compatibility
  module and tests, not App Gateway, TUI, core session logic, and packaging at
  once.
- Build stability: scripts, CI, package metadata, and Bazel targets must use
  Praxis identity unless a public wire contract requires a legacy name.

## Migration Plan

1. Public identity cleanup: remove user-facing Codex product branding from docs,
   package metadata, and primary commands.
2. Add additive Praxis aliases for environment variables, JS globals, and config
   names while preserving only externally observable compatibility aliases.
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
