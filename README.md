<p align="center"><code>npm i -g @openai/praxis</code></p>
<p align="center"><strong>Praxis CLI</strong> is a local agent runtime for coding workflows.</p>

---

## Quickstart

### Installing and running Praxis CLI

Install globally with your preferred package manager:

```shell
# Install using npm
npm install -g @openai/praxis
```

Then run `praxis` to get started.

<details>
<summary>You can also download the appropriate release archive for your platform.</summary>

Each GitHub Release contains many executables, but in practice, you likely want one of these:

- macOS
  - Apple Silicon/arm64: `praxis-aarch64-apple-darwin.tar.gz`
  - x86_64 (older Mac hardware): `praxis-x86_64-apple-darwin.tar.gz`
- Linux
  - x86_64: `praxis-x86_64-unknown-linux-musl.tar.gz`
  - arm64: `praxis-aarch64-unknown-linux-musl.tar.gz`

Each archive contains a single entry with the platform baked into the name (for example, `praxis-x86_64-unknown-linux-musl`), so you likely want to rename it to `praxis` after extracting it.

</details>

### Authentication

Run `praxis` and select **Sign in with ChatGPT**, or configure an API key.

Praxis keeps its runtime state under the Praxis home directory. Read-through support for selected upstream Codex config/auth state is handled as an explicit compatibility path, not as Praxis runtime identity.

## Docs

- [**Contributing**](./docs/contributing.md)
- [**Installing & building**](./docs/install.md)
- [**Configuration**](./docs/config.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).
