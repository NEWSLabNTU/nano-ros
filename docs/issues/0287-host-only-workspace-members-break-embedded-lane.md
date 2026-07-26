---
id: 287
title: "A host-only workspace member silently breaks the embedded clippy lane through cargo feature unification"
status: open
type: tech-debt
area: build
---

## Finding (phase-308, 2026-07-26)

Adding `nros-rmw-metadata` — a host-only crate that deps `nros/metadata-mode`,
which implies `std` — broke `just check-workspace-embedded` with:

```
error[E0463]: can't find crate for `std`
  --> packages/core/nros-serdes/src/lib.rs:31:1
   = note: the `thumbv7em-none-eabihf` target may not support the standard library
```

The crate itself is unreachable from firmware: its self-registration ctor is
`not(target_os = "none")`, and no board or entry deps it. But
`check-workspace-embedded` builds the whole workspace for a thumb target and
cargo unifies features across members, so `nros/std` turned on for everything.
Feature unification does not care what is reachable.

Fixed by adding the crate to the recipe's `--exclude` list, alongside the
other host-only members (`nros-orchestration-ir`, the build-script helpers,
the `-sys` crates).

## Why it is worth tracking

The exclude list is MANUAL and duplicated across two recipes in `justfile`.
Every future host-only crate hits this trap, and the failure points at
`nros-serdes` rather than at the crate that caused it — the diagnostic names
the victim, never the culprit. That is a long debugging session for whoever
adds the next one.

## Options

1. **Derive the exclusion.** Mark host-only crates declaratively — e.g. a
   `[package.metadata.nros] host_only = true` key — and have the recipe build
   the `--exclude` list from a workspace scan instead of a hand-maintained
   literal. Removes the duplication and makes the property live with the crate.
2. **Give host-only crates their own workspace**, as `packages/cli` already
   does. Cleanest isolation (cargo cannot unify across workspaces at all) but
   costs a second lockfile per group and complicates `cargo test --workspace`.
3. **Leave it, add a comment.** Cheapest; keeps the trap.

(1) is preferred: the information belongs on the crate, and the check that
needs it can then never be out of date.
