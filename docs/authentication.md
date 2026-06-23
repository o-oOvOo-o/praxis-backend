# Authentication

Praxis supports ChatGPT sign-in and API key authentication through the login crate.

Authentication state is stored under the resolved Praxis home directory. Selected
Codex-compatible auth/config state may be read through explicit compatibility
bridges, but Praxis runtime state remains isolated under Praxis-owned paths.
