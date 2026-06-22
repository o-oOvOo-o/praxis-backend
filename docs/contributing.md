## Contributing

Praxis contributions should keep the runtime identity, compatibility layers, and product adapters clearly separated.

### Development workflow

- Create a _topic branch_ from `main` - e.g. `feat/interactive-prompt`.
- Keep your changes focused. Multiple unrelated fixes should be opened as separate PRs.
- Ensure your change is free of lint warnings and test failures.

### Guidance for invited code contributions

1. **Start with an issue.** Open a new one or comment on an existing discussion so we can agree on the solution before code is written.
2. **Add or update tests.** A bug fix should generally come with test coverage that fails before your change and passes afterwards. 100% coverage is not required, but aim for meaningful assertions.
3. **Document behavior.** If your change affects user-facing behavior, update the README, inline help (`praxis --help`), or relevant example projects.
4. **Keep commits atomic.** Each commit should compile and the tests should pass. This makes reviews and potential rollbacks easier.

### Model metadata updates

When a change updates model catalogs or model metadata (`/models` payloads, presets, or fixtures):

- Set `input_modalities` explicitly for any model that does not support images.
- Keep compatibility defaults in mind: omitted `input_modalities` currently implies text + image support.
- Ensure client surfaces that accept images (for example, TUI paste/attach) consume the same capability signal.
- Add/update tests that cover unsupported-image behavior and warning paths.

### Opening a pull request

- Fill in the PR template (or include similar information) - **What? Why? How?**
- Include a link to a bug report or enhancement request in the issue tracker
- Run **all** checks locally. Use the root `just` helpers so you stay consistent with the rest of the workspace: `just fmt`, `just fix -p <crate>` for the crate you touched, and the relevant tests (e.g., `cargo test -p praxis-tui` or `just test` if you need a full sweep). CI failures that could have been caught locally slow down the process.
- Make sure your branch is up-to-date with `main` and that you have resolved merge conflicts.
- Mark the PR as **Ready for review** only when you believe it is in a merge-able state.

### Review process

1. A maintainer will be assigned as a primary reviewer.
2. Changes should preserve the Praxis ownership boundaries unless the proposal explicitly changes them.
3. Review focuses on behavior, test quality, dependency direction, and long-term maintainability.
4. When there is consensus that the PR meets the bar, a maintainer will merge it.

### Community values

- **Be kind and inclusive.** Treat others with respect; we follow the [Contributor Covenant](https://www.contributor-covenant.org/).
- **Assume good intent.** Written communication is hard - err on the side of generosity.
- **Teach & learn.** If you spot something confusing, open an issue or discussion with suggestions or clarifications.

### Getting help

If you run into problems setting up the project, would like feedback on an idea, or just want to say _hi_ - please open a Discussion topic or jump into the relevant issue. We are happy to help.

Together we can make Praxis CLI a better agent runtime.

### Security & responsible AI

Use the security contact configured for this repository.
