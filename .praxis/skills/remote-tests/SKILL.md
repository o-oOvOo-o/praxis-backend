---
name: remote-tests
description: How to run tests using remote executor.
---

Some Praxis integration tests support running against a remote executor.
This means that when PRAXIS_TEST_REMOTE_ENV environment variable is set they will attempt to start an executor process in a docker container PRAXIS_TEST_REMOTE_ENV points to and use it in tests.

Docker container is built and initialized via ./scripts/test-remote-env.sh

Currently running remote tests is only supported on Linux, so you need to use a devbox to run them

You can list devboxes via `applied_devbox ls`, pick the Praxis remote test environment.
Connect to devbox via `ssh <devbox_name>`.
Reuse the same checkout of Praxis in `~/code/praxis`. Reset files if needed. Multiple checkouts take longer to build and take up more space.
Check whether the SHA and modified files are in sync between remote and local.
