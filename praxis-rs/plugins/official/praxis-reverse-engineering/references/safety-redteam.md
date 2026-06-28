# Safety Red-Team Track

## Purpose

Praxis needs a way to study safety failures without turning safety research into a bypass tool. This track measures how models and tools behave under approved adversarial fixtures, records failures, and turns the results into remediation work.

## Core Rule

The system may evaluate existing scoped fixtures, but it must not generate, improve, or reveal jailbreak payloads. Reports should use fixture ids, hashes, family labels, expected behavior, observed behavior, and remediation.

False positives are handled through fixture storage and safe projections, not through prompt bypass. A fixture can be evaluated locally by id while the transcript receives only metadata, taxonomy, expected behavior, observed behavior, and remediation.

## Evaluation Record

Each result should include:

- `scope_id`
- `authorization_note`
- `model_provider`
- `model_slug`
- `behavior_profile`
- `fixture_id`
- `fixture_hash`
- `fixture_family`
- `expected_behavior`
- `observed_behavior`
- `passed`
- `severity`
- `remediation`

## Failure Families

Use high-level labels only:

- instruction hierarchy confusion
- role confusion
- policy boundary confusion
- tool permission confusion
- hidden-state disclosure attempt
- unsafe transformation attempt
- unsafe continuation attempt
- deceptive framing
- context injection
- output-format pressure

## Tool Boundary

The first implementation should use structured safety tools rather than shell access:

- plan a scope
- run an existing fixture by id
- record a result
- export an aggregate report

Fixtures should live in an access-controlled artifact store. The transcript should not contain blocked payload text by default.
