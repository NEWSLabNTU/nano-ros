---
id: 1295
title: "A generated Rust workspace entry on CycloneDDS cannot create a publisher — the selection facade never names `nros/rmw-cyclonedds`"
status: resolved
type: bug
area: tooling, rmw-cyclonedds, examples
severity: high
found: 2026-09-11
related: [0831, 0937, rfc-0065, phase-445]
resolved_in: "the facade offers each linked backend's `rmw-X` name to the umbrella too"
---

# What happens

Every generated Rust workspace entry whose image resolves to CycloneDDS opens
its session and then stops at the first node:

```text
[INFO] nros: session open
[ERROR] nros: node declaration failed — NodeError::Transport(PublisherCreationFailed)
nros: application error: NodeRegister("talker_pkg")
```

Measured twice on 2026-09-11, on a tree at origin/main `9b20de043` plus the
phase-445 W5 branch:

1. `nros new probe_rs --workspace --lang rust` → `nros sync` → `nros build` →
   `build/posix/native_entry/target/debug/native_entry`. The scaffold's
   `system.toml` declares `rmw = "cyclonedds"`, so this is the Rust quick start
   in `book/src/getting-started/first-project.md`, as written.
2. The in-tree fixture `workspace-rust-native-cyclonedds`
   (`examples/workspaces/rust`, image `native_cyclonedds`), built by the
   workspace fixture lane with rc=0:
   `examples/workspaces/rust/target-fixtures-cyclonedds/nros-relwithdebinfo/native_cyclonedds_entry`.

The C++ quick start (same scaffold shape, CMake driver) publishes and receives
on CycloneDDS, so the backend itself is fine.

# FIXED — the umbrella takes the marker it declares

The read below was right. The fix is the rule the board dep already used one
block down in `facade.rs`: offer each linked backend's `rmw-X` name to BOTH
crates and let each take it only if it DECLARES it. Nothing is enumerated —
zenoh and xrce have no umbrella feature and stay silent (naming one is a hard
cargo error, not a no-op), a bridge's second backend is covered because the
marker loop reads `image_backends`, and a backend that grows a marker later
needs no edit. `rmw_resolver`'s header claim that "the `nros` umbrella stays
RMW-agnostic" — the sentence that made this invisible — is corrected to
SELECTION-agnostic.

Measured A/B on one tree, `workspace-rust-native-cyclonedds`:

| facade `nros` features | run |
| --- | --- |
| `["rmw-cyclonedds", "ros-humble"]` | `session open` → `application complete` |
| `["ros-humble"]` (rebuilt from the same tree) | `PublisherCreationFailed` |

The zenoh `native_entry` prints the same two lines, so that is this fixture's
healthy shape rather than a silent pass. Repro 1 (the book's Rust quick start,
whose scaffold declares `rmw = "cyclonedds"`) syncs, builds and runs clean.
Controls: the `native` (zenoh) and `native_xrce` rows still emit
`nros = features ["ros-humble"]` with the backend on the board, and both build.

**Still true, and not fixed here:** nothing RUNS a generated Rust workspace
entry in CI — `rmw_coordinate_truth` checks symbols, the matrix cells run the
hand-written per-example binaries, and neither scaffold journey builds a
`--workspace --lang rust` scaffold. That gap is what let this ship; it is worth
its own lane.

# Why (read from the tree, before the fix)

A typed Cyclone publisher needs the `nros` crate's `rmw-cyclonedds` feature:

```toml
# packages/api/nros/Cargo.toml
rmw-cyclonedds = ["nros-node/needs-type-descriptors"]
```

That was one of three hand edits in `book/src/user-guide/rmw-switching.md`'s
old Rust recipe ("the `nros` dependency: add `rmw-cyclonedds`"). Since issue
0831 the selection facade is the ONE place that names the RMW, and it names it
only on the BOARD:

```toml
# generated/nros-selection/native_entry/Cargo.toml (as written by the heal in cmd/build.rs)
nros = { …, default-features = false, features = ["ros-humble"] }
nros-board-linux = { …, default-features = false, features = ["rmw-cyclonedds"] }
```

`orchestration/facade.rs` builds `nros_features` from the ROS edition plus
declared capabilities (`let mut nros_features = vec![edition.cargo_feature()…]`)
and never adds an RMW marker. `nros-board-linux`'s `rmw-cyclonedds` is
`["dep:nros-rmw-cyclonedds-sys"]` — the backend, not the marker — and neither
it nor `nros-rmw-cyclonedds-sys` enables `needs-type-descriptors`. So the entry
links Cyclone and cannot describe its types to it.

The likely fix is for the facade to add the RMW's `nros` marker feature beside
the board's backend feature (zenoh/xrce need none today; check the `nros`
features table). Not attempted here: it is outside phase-445 W5, and the fix
needs its own measurement.

# Why nothing caught it

- `rmw_coordinate_truth.rs` checks the `workspace-rust-native-cyclonedds`
  artifact for Cyclone SYMBOLS (`dds_` present, `_z_` absent). It never runs
  the binary, and the symbols are there.
- The Rust Cyclone matrix cells run the per-example binaries
  (`build_native_rust_example_rmw`), which are hand-written and name the marker
  themselves, not a generated workspace entry.
- `scaffold-journey` scaffolds single-package leaves; `acceptance` scaffolds
  `--use-case talker`. Neither builds or runs a `--workspace --lang rust`
  scaffold. The book probe (`scripts/probe/verify-first-node.sh`) does, and its
  Rust leg is expected red until this is fixed.

This is not a phase-445 W5 regression: W5 did not touch `facade.rs` (empty diff
against origin/main), and the path is the generated Rust entry from #880.
