---
id: 2595
title: "An example migrated to a one-node workspace leaves `linux-check`'s list, and no lane lints it: 32 leaves to 1"
status: open
type: bug
severity: medium
area: [testing, gates, examples]
related: [2380, 1882, 1892, phase-531]
found: 2026-11-15
---

**Home:** [phase-531](../roadmap/phase-531-workspace-first-project-layout.md) — its W6 moves single-node examples into
one-node "witness" workspaces, and this is what that move took out of the lint lane.

## What

`check::linux-check` (`just linux check`, `scripts/build/lint-examples.sh`)
runs `cargo fmt --check` and clippy over every standalone example leaf a
host can lint. Its leaf list passes through
`scripts/lib/drop_workspace_members.py`, which drops every path inside an
`examples/<plat>/<lang>/<name>/` that carries a `.colcon_workspace`
marker. A witness's node package is not a leaf: it has no `system.toml`, and
syncing it as one writes leaf residue. The script says "the witness's fixture
rows build it".

A fixture row BUILDS a package. It does not run `fmt --check` or clippy on it.
As examples migrated (phase-531 W6 from 2026-11-10, and the moves after it),
each one left the lint list and joined no other:

| | leaves `NROS_GATE_LIST_UNITS=1` lists |
| --- | --- |
| `e1eda595e4` (issue 2380's per-leaf split, 2026-11-10) | 32 |
| main, 2026-11-15 | **1** (`mps2-an385-baremetal-rust-listener`) |

The gate still passes, and it now passes over one leaf. Its "no leaf found
is not a pass" guard fires only at zero.

The Rust packages that lost the lint are the 18 under `examples/linux/rust/*`
(talker, listener, the service and action pairs and their `-rtic` variants,
custom-msg, the custom-transport pair, lifecycle-node, logging) and
`examples/mps2-an385-baremetal/rust/talker`: 19 of the 31 that left. The
other 12 are the mps2-an385 bare-metal C and C++ leaves the 2380 profile
measured. They left legitimately: they are now CMake-road components with
no `Cargo.toml` (phase-553 W5), so a cargo lint has nothing to read there.

Sweep, checking whether any other lane runs clippy or fmt over a witness
package:

    git grep -n -E 'clippy' -- just scripts/build | grep -i -E 'workspace|witness|examples'

It finds only the root workspace, the CLI sub-workspace and the runners. None
of them reaches `examples/`.

## Why it matters

This is the 0196 shape: a gate whose reach shrank under a rule that did not.
"Every example compiles warning-free under clippy" is what
`linux-check` asserts, and the examples are the copy-out code users start
from (RFC-0026).

## What would close it

Lint a witness through the road that builds it. Configure the workspace
(`nros configure`, which renders the node package's selection crate and
patch rows), then run `fmt --check` and clippy on each node package with the
image's rendered cargo config, the same shape `lint-example-leaf.sh` gives a
leaf (issues 1882, 1892). One gate unit per witness, so issue 2380's budget
argument still holds.

Raise the guard too. The leaf count should be compared against the tracked
examples it should cover, not against zero.

Acceptance: `NROS_GATE_LIST_UNITS=1` lists every Rust witness above. A
clippy warning planted in `examples/linux/rust/talker` turns
`check::linux-check` red, and the same check passes on main.
