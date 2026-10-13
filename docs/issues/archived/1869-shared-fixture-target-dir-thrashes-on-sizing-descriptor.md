---
id: 1869
title: "Workspace fixture rows that share a target dir rebuild the core crates on every run: each entry points `NROS_SIZING_DESCRIPTOR` at its own descriptor"
status: resolved
type: tech-debt
area: [build, ci]
severity: medium
found: 2026-10-13
resolved: 2026-10-13
resolved_in: "PR #2137"
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

## Resolution (2026-10-13)

Option 1, with the class copy that makes it work. RFC-0100
[Amendment 2 / D13](../../design/0100-rmw-agnostic-sizing-model.md#amendment-2-2026-10-13--d13-a-shared-target-dir-keys-its-build-on-the-descriptors-content):
when `nros build` gets a target dir from its caller
(`builder::shared_target`), each image builds in cargo's `build.build-dir` at
`<target-dir>/nros-class/<key>/build/`, `<key>` hashing the image's `[env]`
rows, the board template and triple, and the descriptor's CONTENT; the
descriptor is named to cargo as a class copy at
`<target-dir>/nros-class/<key>/sizing.toml` (`[meta] entry = "class-<key>"`,
written only when it changes). Equal configurations share one path and one
build dir, so 1623's text edge never fires between them; different ones never
share a unit. Binaries are still uplifted to `<target-dir>/<profile>/`, so no
test-side locator moved. Option 2 alone would not have been enough — the
facts differ (`model_nodes` 2 vs 1 reaches `ZPICO_MAX_LIVELINESS` 19 vs 18), so
a shared unit had to be either rebuilt or reused under the wrong config.

Measured on the box, `scripts/build/workspace-fixtures-build.sh linux rust`
(24 rows), warm, no change, run twice:

| | rows that compiled | row time |
| --- | --- | --- |
| before | 19 | 273 s (whole script 281 s) |
| after | 0 | 30 s (whole script 34 s) |

Two of the "before" rows (`workspace-rust-native-bridge`,
`workspace-rust-native-cyclonedds`) were a separate defect found on the way:
`cyclonedds-sys/build.rs` watched its own `OUT_DIR`, which its cmake build
writes, so it was dirty after every run. The watch is deleted in the same PR.

Correctness, same tree: the rust workspace's eight `target-fixtures` rows fall
into two classes (six 1-node images, two 2-node), and the two classes' zpico
`shim_constants.rs` state `ZPICO_MAX_LIVELINESS = 18` and `19`. Switching
`[image.native]` to the 1-node launch made the row's `.inputsig` disagree with
the signature before any build (STALE), and the rebuild moved `native` into the
1-node class, compiling only its entry crate (2 s); reverting moved it back,
and the uplifted binary was byte-identical to its own class's `deps/` copy each
time.

Disk: one build tree per distinct configuration (about 0.55–0.6 GiB each in
the rust workspace) instead of one per target dir. A tree built before this
change keeps its old `<target-dir>/<profile>/{deps,build,…}` intermediates
until cleaned; a class whose configuration changed leaves its tree behind the
same way (the CI fixture tree's 7-day reset bounds both).

Not covered: the cmake/Corrosion workspace rows (`mixed`, `features` C/C++)
name the D12 runtime descriptor, one per configure, and are not touched here;
their per-row cmake reconfigure is phase-489 W6's other item. Single-package
`[[fixture]]` leaves in the phase-340 group dirs build with `cargo` directly,
not through `nros build`; in the measured tree their descriptor consumers already
have distinct unit hashes per leaf, so they do not collide; a leaf pair that
did would need this same class step on that road.
