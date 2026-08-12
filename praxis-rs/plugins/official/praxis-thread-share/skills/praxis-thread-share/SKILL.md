---
name: praxis-thread-share
description: Share the current real Praxis thread to the configured private Git repository. Use when the user asks to publish, share, or sync the current conversation, or explicitly invokes /share.
---

# Praxis Thread Share

Use `/share <team>` for the current thread. Quote team names containing spaces,
for example `/share "Geometry Core"`. The current GitHub repository is the
Project; equal normalized team names inside that project share one room. The command
receives its thread id, rollout path, and working directory from Praxis App
Gateway context; do not ask the user to locate a rollout file manually.

The canonical exporter includes only real user and assistant messages. It
excludes bootstrap instructions, reasoning records, tool arguments, tool
outputs, and absolute local paths. Credential-like content is redacted before
any file is staged. Never bypass the redaction pass or commit a raw JSONL
rollout.

On success, report the Git commit and link returned by the command. On failure,
show the exact configuration or Git error and leave the source rollout
unchanged. Do not fabricate a successful share.
