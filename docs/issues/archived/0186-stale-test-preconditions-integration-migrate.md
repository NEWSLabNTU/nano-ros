---
id: 186
title: "test rot: integration shell smokes probe retired layouts; migrate_workspace gated on lagging nros release pin"
status: resolved
type: tech-debt
area: testing
related: [issue-0164]
resolved_in: "2026-07-13 deletions (3 smokes + the migrate verb), maintainer-decided"
---

## Summary (as filed)

Four deterministic reds whose preconditions were stale: the
zephyr/esp-idf/platformio integration shell smokes probed layouts retired in
Phase 208.D.7/D.8/D.10 (each test's own comments were an epitaph explaining
why its probe could never pass again), and `migrate_workspace_e2e` sat behind
a "release pin lags the emitter spec" drift gate.

## Resolution (maintainer call, 2026-07-13)

**The three smokes: deleted.** Their assertions validated file shapes that no
longer exist anywhere in the tree; the canonical shapes are covered by
`cli_bringup_{zephyr,esp_idf,platformio}` (all green as of #185) and the west
fixtures. `integration_nuttx` / `integration_px4` stay (their layouts are
live). Registrations scrubbed: `[[test]]` blocks, the justfile `env_exclude`
filters, the nextest `integration_esp_idf` slow-timeout override, the
platformio.just comment.

**The `nros migrate workspace` verb: deleted entirely** (the stronger option,
chosen over retiring just the e2e). The "release pin" framing was doubly
obsolete — post-218 there is no release pin (the test ran the per-checkout
CLI), and the in-tree emitter itself never adopted the post-212.I
`[package.metadata.nros.component]` sub-table, so the gate was a tautology
that could never flip. Removed: `cmd/migrate.rs`, the hidden `Cmd::Migrate` /
`MigrateSub` surface, the dispatch arm, and all four migrate tests.

**Breaking-removal note:** a pre-212 workspace no longer has an in-tree
migration path. The `NrosTomlNotSupported` diagnostic (and RFC-0004 /
configuration.md / the workspace-metadata schema doc) now direct such trees
to run the verb from the **`nros-v0.5.0` tag's CLI** — the last tag that
carries it — or start fresh from `nros new system <bringup>`.

Verified: CLI rebuilds; `nros migrate` → "unrecognized subcommand";
`nros_config` unit tests 12/12 (the diagnostic still names the migration
path); `integration_nuttx`/`integration_px4`/`example_shape` 11/11; nextest
lists no deleted binaries; clippy no new warnings; nightly fmt clean.
