---
name: praxis-token-saver
description: Analyze Praxis token savings and estimated API input cost for this month, this week, all time, or an exact date range. Use when the user asks how many tokens or how much money Praxis saved, requests a Token Saver report, wants savings broken down by model, category, day, or thread, or wants periods compared.
---

# Praxis Token Saver

Use the canonical read-only report instead of deriving figures from the TUI or rollout files.

## Get the data

Resolve relative dates in the user's timezone. When the user gives no range, use the current calendar month in the machine timezone.

Run one of:

```text
praxis token-saver --period month --format json
praxis token-saver --period week --format json
praxis token-saver --period all --format json
praxis token-saver --from <RFC3339> --to <RFC3339> --utc-offset-minutes <minutes> --format json
```

For comparisons, run one exact half-open range per period. Never guess missing prices or replace the report's figures with TUI values.

## Present the report

Answer in the user's language. Lead with total saved tokens and estimated saved cost, then show compact Markdown tables for model, category, day, and top threads when those sections contain data. Use thousands separators and sensible currency precision.

Always say that cost is an API list input-price estimate, not an invoice. If `unpriced_saved_tokens` is nonzero, clearly state that those tokens are excluded from the cost estimate. For comparisons, show absolute and percentage changes only when both periods have valid data.

If the command fails or no canonical data is available, explain that plainly and do not fabricate a report.
