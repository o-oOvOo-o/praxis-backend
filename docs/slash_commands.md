# Slash commands

Praxis slash commands are parsed by the TUI and routed to session, workspace,
model, plugin, or external-thread actions.

The `/codex` command is reserved for the Codex thread-compatibility
bridge and should not be used for generic Praxis behavior.

`/namegen` runs while a task is active. It submits a background thread-name
request without queuing a user turn or waiting in the UI event loop. The thread
context menu exposes the same action as Generate name. A thread controlled by
another agent remains locked, and repeated requests for the same thread are
coalesced while name generation is in progress.

The gateway tracks name generation as background work so its dispatcher remains
available for other requests, including interrupting the active turn.
