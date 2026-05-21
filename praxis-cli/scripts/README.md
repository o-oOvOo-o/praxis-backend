# npm releases

Use the staging helper in the repo root to generate npm tarballs for a release. For
example, to stage the CLI, responses proxy, and SDK packages for version `0.6.0`:

```bash
./scripts/stage_npm_packages.py \
  --release-version 0.6.0 \
  --package praxis \
  --package praxis-responses-api-proxy \
  --package praxis-sdk
```

This downloads the native artifacts once, hydrates `vendor/` for each package, and writes
tarballs to `dist/npm/`.

When `--package praxis` is provided, the staging helper builds the lightweight
`@openai/praxis` meta package plus all platform-native `@openai/praxis` variants
that are later published under platform-specific dist-tags.

If you need to invoke `build_npm_package.py` directly, run
`praxis-cli/scripts/install_native_deps.py` first and pass `--vendor-src` pointing to the
directory that contains the populated `vendor/` tree.
