---
id: 409
title: setup-launch-resolve exits 0 when the submodule is missing, so a stale resolver silently strips every model's hand-authored params
status: open
type: bug
area: tooling
related: [0380, 0285, 0363, rfc-0060, rfc-0063, phase-330, phase-332]
---

## Problem

`just setup-launch-resolve` returns **0** without building when
`packages/cli/third-party/play_launch` is uninitialised:

```
[setup-launch-resolve] SKIP: play_launch submodule not initialised
  git submodule update --init packages/cli/third-party/play_launch
```

Nothing downstream treats that as a problem. `nros sync` then resolves with
whatever `nros-launch-resolve` binary is left on disk — here one built on
2026-08-02, before phase-332 repointed at the play_launch repo — and that binary
predates `apply_params_to_nodes` (rlm v0.1.1).

The result is silent data loss of exactly the kind issue 0380 exists to prevent:
**every `[[component]].params` and `params_files` declaration disappears from the
generated model.** No error, no warning, exit 0.

Found by `just ci-matrix`: 25 failures / 2 timeouts, most of them clustered in
`examples/workspaces/features`, e.g.

```
qos_override_e2e the_committed_model_declares_a_reliability_override_that_lowers
  panicked: the fixture's whole point is this override; model params: {}
```

The declaration was right there in `system.toml`:

```toml
params = { "qos_overrides./qos_chatter.publisher.reliability" = "best_effort" }
```

With the submodule initialised and the resolver rebuilt, the same `nros sync`
produces the params — 22 model files in `features/` alone changed, +108 lines.
The committed models were wrong in the repository, so this was not only a local
condition.

## Why the existing guards miss it

- `check-model-dims` watches `execution.tiers` dims, not `structure.nodes[].params`.
  A model can lose every param and the gate stays green.
- The nano-ros side parses `params` / `params_files` only so its deny-unknown-fields
  parser does not reject the bringup — a comment in
  `cargo_metadata_schema.rs` says so explicitly and points at the resolver. Nothing
  checks that the resolver actually performed the projection.
- `just doctor` did not flag the missing submodule or the stale binary.
- `setup-cli` warns when the resolver is older than the CLI. That warning fired in
  this session and was satisfied by running `setup-launch-resolve` — which
  skipped, printed SKIP, and exited 0, so the warning looked addressed while
  nothing had been rebuilt.

The stale-CLI guard (issues 0363/0197) refuses to run a stale `nros`. Its sibling
does not exist for the resolver: a stale or absent resolver is not a build
failure, it is a quiet content change in generated files.

## PARTIALLY FIXED (2026-08-04) — directions 1 and 4

**1. The recipe fails instead of skipping.** A missing submodule is exit 1 with
the init command. `NROS_ALLOW_NO_LAUNCH_RESOLVE=1` keeps the CLI-only path the
`setup-cli` comment worries about, and DELETES any binary left behind — an
opt-out that leaves a stale resolver on disk would preserve the exact hazard,
so the skip now makes a later `nros sync` fail loud on a missing resolver.

**4. `just doctor` checks freshness, not existence.** It reported `[OK]` for any
binary that existed, which is how a stale one stayed invisible. It now walks the
resolver sources (`git -C` the submodule, since the superproject index holds
only the gitlink) and reports `[STALE]` with the remedy.

Verified: submodule absent -> exit 1 + remedy; with the override -> exit 0 and
the binary removed; submodule present -> builds, doctor `[OK]`; source touched
-> probe names `resolve/Cargo.toml`, and `[OK]` again after a rebuild.

**Still open — 2 and 3.** `nros sync` does not yet verify the resolver it is
about to run. Direction 3 needs restating: phase-330 W7.e BANNED committed
models (`check-no-tracked-models`), so there is no committed artifact left to
gate — the equivalent watcher is a post-resolve assertion inside `nros sync`
that every `params` / `params_files` declaration in `system.toml` either appears
in the model or is reported as an explicit unbound diagnostic. The side finding
below (component names not matching launch node names) means "absent from the
model" is sometimes legitimate, so that check must distinguish the two rather
than fail on any absence.

## Fix directions

1. **`setup-launch-resolve` must fail, not skip**, when it cannot build — or at
   minimum leave a marker the consumers check. A recipe whose job is "produce a
   binary" and which exits 0 without producing one is the defect.
2. **`nros sync` should refuse a resolver it cannot verify**, the way the CLI
   refuses a stale self. The resolver's version/pin is knowable
   (`play_launch` submodule SHA); compare and fail loud.
3. **Extend the 0380 gate to `structure.nodes[].params`.** The dims baseline
   proved its worth twice; params are the same class of hand-authored,
   resolver-unreproducible content and currently have no watcher.
4. `just doctor` should check the play_launch submodule the way it checks other
   provisioned sources.

## Side finding, not the cause

`features/`'s launch node names do not match their `[[component]].name`s (23
nodes; W2b prefixed the components for workspace-wide uniqueness, the launch
files kept the bare names). The correct resolver reports this as a diagnostic —
`'rust_qos_reliable_talker' declares params but has no matching launch node
(absent in this variant?)` — and still binds within each variant, so it is not
the failure above. It is noise worth cleaning up, and it made the real cause
harder to see: the mismatch is a plausible-looking explanation that turns out to
be wrong.
