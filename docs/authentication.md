# Authentication

Praxis supports ChatGPT sign-in and API key authentication through the login crate.

Authentication state is stored under the resolved Praxis home directory. Selected
Codex-compatible auth/config state may be read through explicit compatibility
bridges, but Praxis runtime state remains isolated under Praxis-owned paths.

The `/login` menu separates account authentication from API interfaces:

- ChatGPT/Codex account login and Codex/OpenAI API key login share the `openai`
  provider and its registered model catalog. API key login defaults to the
  official OpenAI Responses URL, while allowing the URL to be configured.
- Claude Pro/Max OAuth and Anthropic API key login share the `anthropic`
  provider and its registered model catalog. API key login defaults to the
  official Anthropic URL, while allowing the URL to be configured.
- `responses-api` and `claude-api` are separate custom providers for endpoints
  whose model name must be entered explicitly.

Provider API keys are stored in the operating system credential store. Praxis
configuration stores only provider metadata and the credential reference.
