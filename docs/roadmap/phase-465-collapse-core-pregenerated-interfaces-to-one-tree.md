# phase-465 - collapse the core pre-generated interface set to one output tree

**Status (2026-09-22). PROPOSED; nothing landed. Decided but not started.**
Implements RFC-0067 §D5. Closes issue 1428. The decision is made — one output
tree, not a shared canonical crate — and is argued and measured in the RFC; this
document is the work and its acceptance. Not affordable in the session that
decided it: the migration is a ~25-file rename with lockfile churn whose only
honest acceptance is a build, and CLAUDE.md's issue-0394 history is a half-applied
rename of exactly this shape breaking a fresh clone, twice.

## What is there now

Four driver packages under `packages/interfaces/`, four output trees, **eight**
tracked crates:

| driver package | tracked crates |
| --- | --- |
| `rcl-interfaces/` | `nros-rcl-interfaces`, `nros-builtin-interfaces` |
| `diagnostic-msgs/` | `nros-diagnostic-msgs`, `nros-std-msgs-diag`, `nros-builtin-interfaces-diag` |
| `rosgraph-msgs/` | `nros-rosgraph-msgs`, `nros-builtin-interfaces-clock` |
| `lifecycle-msgs/` | `nros-lifecycle-msgs` |

`builtin_interfaces` appears three times with **byte-identical** Rust sources
(re-verified 2026-09-22: one md5 each for `src/lib.rs`, `src/msg/mod.rs`,
`src/msg/time.rs`, `src/msg/duration.rs` across all three), all three declaring
`TYPE_NAME = "builtin_interfaces/msg/Time"`. The `-diag` / `-clock` suffixes are
`--rename` flags in the `justfile` whose only job is to stop three crates named
`nros-builtin-interfaces` colliding in one workspace.

## The end state

**One** driver package, **one** output tree, **six** crates: `nros-rcl-interfaces`,
`nros-diagnostic-msgs`, `nros-std-msgs`, `nros-rosgraph-msgs`,
`nros-lifecycle-msgs`, `nros-builtin-interfaces`. Every dep a flat sibling
(`path = "../<dep>"`), every crate `nros-`prefixed, `links` unique because there
is one copy of each ament package.

**No codegen change is required.** Measured 2026-09-22 (with the parent
checkout's release `nros`, this worktree having none — the phase's own acceptance
re-measures with a locally built CLI): one driver `package.xml` depending on
`rcl_interfaces`, `diagnostic_msgs`, `rosgraph_msgs` and `lifecycle_msgs`, one
`generate-rust -o out` with six `--rename`s, emits

```
Generating bindings for 7 interface packages...
  ✓ builtin_interfaces (2 messages, 0 services, 0 actions)     ← once
  ✓ std_msgs (30 messages, 0 services, 0 actions)              ← once
  ✓ diagnostic_msgs, geometry_msgs, lifecycle_msgs, rcl_interfaces, rosgraph_msgs
```

with `nros-rosgraph-msgs` naming `nros-builtin-interfaces = { path =
"../nros-builtin-interfaces" }` and `nros-diagnostic-msgs` naming
`nros-std-msgs = { path = "../nros-std-msgs" }`. One invocation emits each ament
package exactly once because `resolve_transitive_dependencies` returns a
`HashSet` and `filter_interface_packages` iterates it. Sources match the
committed ones modulo `cargo fmt`.

`geometry_msgs` arrives in the closure (via `diagnostic_msgs`' ament deps),
unrenamed and unused; the current `generate-diagnostic-msgs` recipe already
`rm -rf`s it and the merged recipe keeps doing so. Note it is the one emitted
crate that would ship an **unprefixed** ament name if it were ever kept —
W4 asserts it is not.

## Waves

### W1 — one driver package, one recipe

- One `package.xml` with four `<depend>` rows, replacing the four per-package
  ones. Decide its location: `packages/interfaces/package.xml` with the output
  at `packages/interfaces/generated/<edition>/`, keeping the per-package
  directories only if something still needs them (see W5 — the cmake layer-3
  bundled lookup reads `packages/interfaces/<pkg>/package.xml`; check whether it
  ever matches, since the directories are hyphenated and ament names are not).
- Merge the RFC-0033 capacity configs into one `nros-codegen.toml`. Lossless:
  keys are package-qualified, and one invocation builds one `CapacityResolver`
  for the whole closure. Today only `diagnostic-msgs/` has one.
- Four private `generate-*` recipes → one. `just generate-bindings` keeps its
  name. The recipe drops `geometry_msgs` and `cargo fmt`s the tree, as today.
- Retire the documented "generate out of tree and copy only `src/lib.rs` back"
  procedure (`4b80db633`): with one tree and one emitted manifest shape there is
  nothing hand-maintained left to clobber.

### W2 — move the six crates, delete the two duplicates

`git mv` the six into the new tree; delete `nros-builtin-interfaces-diag` and
`nros-builtin-interfaces-clock`. Reshape the three hand-adapted manifests
(`nros-builtin-interfaces`, `nros-rcl-interfaces`, `nros-lifecycle-msgs` — the
pre-0394 vintages that still carry `edition.workspace`, `license.workspace`,
`description`, `[package.metadata.ros]` and `.workspace = true` dep rows) to the
emitted shape, so a regeneration is in-place and idempotent. Their `version` is
already the `0.0.0` constant (PR #1144).

**Do not half-apply this.** A shared crate wired for two of three parents is
worse than either end state.

### W3 — consumers and locks

- Root `Cargo.toml`: eight member lines → six. Also resolve the two excluded
  metadata-only parent crates (`rcl-interfaces` v0.4.0, `lifecycle-msgs` v0.4.0
  — `[package]`s with no Rust targets) — either they move with the driver
  package or they go.
- ~19 dep rows: `packages/testing/nros-tests` (6),
  `nros-tests/bins/{sim-clock-listener,sim-clock-publisher,contract-monitor}`
  (7), `packages/rmw/cyclonedds/nros-rmw-cyclonedds` (2),
  `packages/core/nros-node` (3), `packages/core/nros-diagnostics` (1). The
  `-diag` / `-clock` crate NAMES disappear, so every row naming one changes its
  key as well as its path.
- Locks via `just lock-update` **only** — never a bare `cargo generate-lockfile`.
  The root lock plus the tracked leaf locks under `nros-tests/bins/*`.

### W4 — the gate and the baseline

`.config/duplicate-wire-type-baseline.txt` holds exactly the six claims of the
triple (`Time` and `Duration` × three crates). Rule 3 of
`check-message-crate-identity` is a shrink-only ratchet, so a baselined duplicate
that stops duplicating fails as **stale** — the collapse must empty that file in
the same commit, which is the ratchet doing its job. Assert afterwards that the
shipped duplicate population is zero and that no emitted crate carries an
unprefixed ament name.

Also correct the gate's own comment in `just/check/cargo.just`, which says
collapsing "is a codegen change" — true of a shared canonical crate, false of
this shape.

### W5 — the reach sweep

Per CLAUDE.md's fix-the-class rule, grep every sibling of the old paths before
declaring done, not just the manifests:

- `cmake/compat/stubs/_NrosFindRosMsgPackage.cmake` layer-3 bundled lookup
  (`packages/interfaces/<pkg>/package.xml`);
- `packages/cli/nros-cli-core/src/cmd/{ws.rs,build.rs}`, which enumerate
  `packages/interfaces/`;
- `packages/testing/nros-tests/tests/schema_serializer_round_trip.rs`, which
  asserts its corpus row count **against what `packages/interfaces` defines** —
  removing two crates shrinks that count, so this test fails until the corpus
  moves with it. Expected, and it is the test that proves the sweep;
- `packages/testing/nros-tests/tests/serialized_size_bound.rs` (64 types);
- `just/zephyr-{ci,dev}.just` `NROS_BUILTIN_INTERFACES_DIR`;
- docs and RFCs naming the old paths (`just check doc-refs`,
  `just check markdown-links`).

## Acceptance

A build, not a gate:

- `just check test-targets` (workspace-wide and per crate). `cargo build
  --workspace` is **not** a valid command in this tree — three crates define
  `#[panic_handler]`, so it fails `E0152`.
- `just check leaf-lockfiles`, `just check message-crate-identity`,
  `just check cargo-config-tracked`, `just check fast`.
- `just ci gate`.
- `just generate-bindings` on an ament host, twice: the second run must be a
  no-op against the tree the first produced (in-place idempotence is the property
  W1/W2 buy, and the thing the four-tree layout never had).
- `nros-tests` no longer path-deps two copies of one wire type.

## What this phase does NOT do

**The `links` rename** (issue 1455, RFC-0067 §D4). `links` is derived from the
ament package name and the rename pass does not rewrite it, so a shipped crate
collides with a consumer's own copy of the same ament package at resolve time.
The collapse does not fix that — it reduces nano-ros to one copy, while the
consumer still has theirs — and 1455 does not block this phase either. They are
independent, and 1455 is the cheaper and more urgent of the two.

**Multi-edition.** The tree stays `generated/humble/`. A second edition is a
second tree and a legitimate duplicate under §D5; it is not in scope here.

## Owns

`packages/interfaces/**`, root `Cargo.toml` member list, the `generate-*` block
in `justfile`, `.config/duplicate-wire-type-baseline.txt`, the
`message-crate-identity` comment in `just/check/cargo.just`, and the dep rows
named in W3.
