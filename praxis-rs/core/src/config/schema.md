# Config JSON Schema

We generate a JSON Schema for `~/.praxis/config.toml` from the `ConfigToml` type
and commit it at `praxis-rs/core/config.schema.json` for editor integration.

When you change any fields included in `ConfigToml` (or nested config types),
regenerate the schema:

```
just write-config-schema
```
