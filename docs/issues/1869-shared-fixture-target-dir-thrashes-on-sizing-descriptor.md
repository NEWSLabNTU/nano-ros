---
id: 1869
title: "Workspace fixture rows that share a target dir rebuild the core crates on every run: each entry points `NROS_SIZING_DESCRIPTOR` at its own descriptor"
status: open
type: tech-debt
area: [build, ci]
severity: medium
found: 2026-10-13
related: [phase-489, issue-0491, issue-1623, issue-0616, rfc-0100, rfc-0070]
---

## Measured

Found by phase-489 W6, re-running `just build native` with no source change on
a warm tree (4-core box). Rows of one workspace that share a cargo target dir
rebuild the same crates every run, in whatever order the pool runs them:

    workspace-rust-native                 2 s   (consecutive re-run: fresh)
    workspace-rust-native-service-server  15 s  nros-params nros-rmw-cffi zpico-sys nros-node
                                                nros nros-rmw-metadata nros-rmw-zenoh
                                                nros-board-linux …
    workspace-rust-native-service-server  2 s
    workspace-rust-native                 14 s  the same crates again

Over the native lane, 26 of 154 rows compiled something on a no-change re-run,
about 390 s of row time.

## Cause

`examples/fixtures.toml` groups these rows into one target dir per workspace
(phase-340): `examples/workspaces/rust` (8 rows in `target-fixtures`),
`realtime-rust` (2), `mixed` (7), `features` (5 + 4), `safety` (2). Each entry's
generated `nros-cargo.toml` differs only in

    NROS_SIZING_DESCRIPTOR = { value = "<entry>/nros/sizing/<image>.toml", relative = true }

and every build script that reads the descriptor emits
`cargo::rerun-if-env-changed=NROS_SIZING_DESCRIPTOR` (issue 1623, which needed the
unset→set edge). Cargo compares that value as TEXT, so a different entry is a
different input to the same unit — issue 0491's shape. The contents differ too
(`entry`, `model_nodes`), so the rebuild is not spurious for every consumer:
one unit identity is carrying two configurations, which is issue 0616's shape.

The user's road does not have this: `nros build <bringup>:<image>` writes
`target-dir = "<entry>/target"` per image into `nros-cargo.toml`, and only the
fixture rows override it with the group dir.

## Options

- Key the group dir on the descriptor CONTENT that reaches a build script, so
  equal configurations share and different ones do not (the
  `nros_shared_cargo_dir` rule, RFC-0094 D4).
- Have each consumer watch only the descriptor facts it reads, so an entry
  differing in a fact a crate never reads does not rebuild it.
- Drop the override for these rows and accept per-image dirs (more cold
  compile; sccache recovers the crates the descriptor does not reach).

Not done in phase-489 W6: each changes the RFC-0070/RFC-0100 contracts the
group dirs and the descriptor carrier rest on.
