The messages above are new conversation messages to incorporate into the existing summary provided in <previous-summary> tags.

Update the existing structured summary with the new information. Rules:
- Preserve all still-relevant information from the previous summary.
- Add new progress, decisions, constraints, and critical context from the new messages.
- Update the Progress section by moving completed work from In Progress to Done.
- Update Next Steps based on what was accomplished and what remains.
- Preserve exact file paths, function names, command output snippets, and error messages when they matter.
- Remove information only when the new messages make it clearly obsolete.

Use this exact format:

## Goal
[Preserve existing goals and add new goals if the task expanded.]

## Constraints & Preferences
- [Preserve existing constraints and add new ones discovered.]

## Progress
### Done
- [x] [Previously completed work and newly completed work.]

### In Progress
- [ ] [Current work after the new messages.]

### Blocked
- [Current blockers, if any.]

## Key Decisions
- **[Decision]**: [Brief rationale.]

## Next Steps
1. [Updated ordered list of what should happen next.]

## Critical Context
- [Preserved important context and newly discovered context.]

Keep each section concise.
