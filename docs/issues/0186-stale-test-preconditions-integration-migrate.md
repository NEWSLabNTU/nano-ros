---
id: 186
title: "test rot: integration shell smokes probe retired layouts; migrate_workspace gated on lagging nros release pin"
status: open
type: tech-debt
area: testing
related: [issue-0164]
---

## Summary

Deterministic fail-louds whose PRECONDITIONS are stale, not the product:

- `integration_zephyr::zephyr_integration_shell_smoke`:
  `[SKIPPED] integrations/zephyr/module.yml absent — Phase 208.D.7 folded the
  shell into zephyr/ (canonical module lives at zephyr/module.yml)` — the
  test still probes the pre-208.D.7 path. `integration_esp_idf` +
  `integration_platformio` shell smokes fail the same way (retired
  `integrations/` layout).
- `migrate_workspace::migrate_workspace_e2e`:
  `[SKIPPED] installed nros migrate workspace does not yet emit
  [package.metadata.nros.component] — Phase 214.N drift gate (the nros-cli
  release pin lags the post-212.I emitter spec)` — by-design drift gate that
  stays red until the pinned nros release is bumped.

## Fix direction

Point the three integration smokes at the folded locations (or delete them if
the folded copies are covered elsewhere); bump the pinned nros release (or
relax the migrate gate to skip-quietly when the pin is known-old). Repo rule:
`skip!` is correct for unmet preconditions, but a precondition that can never
be met again is test rot, not a gate.
