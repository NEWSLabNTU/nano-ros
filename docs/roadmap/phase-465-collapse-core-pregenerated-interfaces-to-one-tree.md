# phase-465 - collapse the core pre-generated interface set to one output tree

**Status (2026-09-25). LANDED — all five waves. Closes issue 1428 (archived).**
Executed on `feat/phase-465-one-interface-tree`, five commits, one per wave. The
acceptance below is met; what actually happened, including the three things this
document predicted wrongly, is in "What landed" at the end.

*(Original framing, 2026-09-22, kept because the argument is still the argument:)*
**PROPOSED; nothing landed. Decided but not started.**
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

**The `links` rename** (issue 1455, RFC-0067 §D4) — **already landed, 2026-09-24,
ahead of this phase and independently of it.** `links` was derived from the ament
package name and the rename pass did not rewrite it, so a shipped crate collided
with a consumer's own copy of the same ament package at resolve time. The
collapse would not have fixed that — it reduces nano-ros to one copy, while the
consumer still has theirs. `apply_package_renames` recomputes the key from the
final crate name now; a crate this phase renames gets a correct one for free, and
nothing here needs to re-decide it.

**Multi-edition.** The tree stays `generated/humble/`. A second edition is a
second tree and a legitimate duplicate under §D5; it is not in scope here.

## Owns

`packages/interfaces/**`, root `Cargo.toml` member list, the `generate-*` block
in `justfile`, `.config/duplicate-wire-type-baseline.txt`, the
`message-crate-identity` comment in `just/check/cargo.just`, and the dep rows
named in W3.

## What landed — 2026-09-25

Five commits on `feat/phase-465-one-interface-tree`, one per wave. **Eight
crates became six; four driver packages, four output trees and four private
recipes became one of each.**

| wave | what |
| --- | --- |
| W1 | one `package.xml` (four `<depend>` rows), one `nros-codegen.toml`, one `just generate-interfaces` |
| W2 | `git mv` the six, delete `-diag` / `-clock` and the two metadata-only parents, regenerate in place |
| W3 | root members 8 → 6, ~19 dep rows + the Rust `use` paths, four tracked locks |
| W4 | the duplicate baseline empties; rule 5 (`nros-` prefix) added with a negative control |
| W5 | the reach sweep — two dead lookups and one silently-broken gate |

### The acceptance

- **`just generate-interfaces` twice**, the run this phase is really about:
  after each run `git status --porcelain packages/interfaces` is EMPTY. In-place
  idempotence — the property four trees never had.
- `just check test-targets` → `clippy + test targets clean (root + cli).`
- `just check message-crate-identity` → `OK (6 generated crate(s), 6 declaring
  links, 324 manifest(s), 0 baselined duplicate claim(s))`
- `just check leaf-lockfiles` → `leaf lockfiles OK`
- `just check cargo-config-tracked` → `OK (tracked <=> hand-authored content)`
- `just check generated-schema-coverage` → `OK (63 message struct(s) carry
  FIELDS, 63 serializer(s) wrap a DHEADER)`, after W5 repaired its pathspec
- `just check fast` → 354 gates green
- `just ci gate` → green

Lock movement, via `just lock-update` ONLY: **17 insertions / 44 deletions**
across the root lock and `bins/{contract-monitor,sim-clock-listener,
sim-clock-publisher}`. Every line a renamed or removed package; nothing
re-resolved.

### The codegen-vintage decision — TAKEN, and it was not optional

Regenerating moves `NROS_EMITTED_CODEGEN_VERSION` 2 → 7, gives the four crates
that lacked one a `links` + `build.rs` + `nros_message_bounds.json` bounds
channel, adds `cyclone_schema_shape` to the bounds JSON of the two that had one,
moves `ament_version` 1.2.1 → 1.2.2 on this host's ROS, and **fixes mojibake** —
three `nros-diagnostic-msgs` sources carried UTF-8-read-as-latin-1 comment
banners (`ââ … â`). Issue 1455's agent hit this drift, reverted and flagged it,
which was right for that phase.

Pinning the vintage was considered and is **impossible here**: the acceptance IS
in-place idempotence, and a committed tree that differs from what the recipe
emits is a tree the second run rewrites. The regeneration is part of the
collapse, not an extra alongside it. It is also cheap — after `rustfmt`, **4 of
60 source files** differed for any reason other than the version constant and the
renamed sibling crates.

### What this document got wrong

- **"`just generate-bindings` on an ament host, twice"** (Acceptance) names the
  wrong recipe. `generate-bindings` drives the EXAMPLES and is a dependency of
  `_codegen`, i.e. of every build; wiring the TRACKED interfaces tree into it
  would rewrite tracked files on every build (re-staling every fixture) and make
  a ROS host a precondition for building anything at all. W1's own text — "four
  private `generate-*` recipes → one; `just generate-bindings` keeps its name" —
  is the consistent reading, and the merged recipe is `just generate-interfaces`.
  The idempotence property is unchanged; only the verb is.
- **W3's "~19 dep rows"** is right in count and incomplete in kind: the Rust
  `use` paths move too (7 files under `nros-tests` and its bins), `nros-tests`
  needed a row REMOVED (it path-depped two `builtin_interfaces`), and a
  different one ADDED (below).
- **W5 predicted `schema_serializer_round_trip` "fails until the corpus moves
  with it"**. It would have — but it was ALREADY failing, for a different
  reason. `corpus_is_exhaustive` counts every `impl ::nros_serdes::Message`
  under `packages/interfaces` and asserts the corpus matches; phase-425 W2 added
  the rosgraph tree (3 impls) and no corpus row, so it has read **76 against
  79** ever since, i.e. `rosgraph_msgs/msg/Clock` was never round-tripped by the
  sweep that exists to round-trip everything. Fixed with a `nros-rosgraph-msgs`
  dev-dep and one `entry!` row: 75 against 75.

### The sweep found two more dead things, both predating this phase

- **`check-generated-schema-coverage` matched NOTHING** after the move — its
  pathspec `packages/interfaces/**/generated/**/*.rs` required a driver-package
  component before `generated/`. Its own empty-list precondition reported that,
  instead of a green run over zero files.
- **The cmake layer-3 `packages/interfaces/<pkg>` rung could never match** —
  W5's own question, answered: `${pkg}` is an ament name and those were four
  hyphenated DRIVER packages, none carrying `msg/` either, so even a name match
  would have found no IDL. Removed, with the file header and the
  `workspace-shadowing` template corrected to name the rung that does answer
  (`packages/cli/interfaces/`).
- **`cmd/build.rs`'s enumerator resolved nothing**, same cause: it read
  `packages/interfaces` itself and stripped an `nros-` prefix none of those
  directories carried, contributing `rcl-interfaces` where a `<depend>` says
  `rcl_interfaces`. Fixed to read one level down and undo both halves of the
  rename; the collapse to one tree is what makes that path expressible at all.
- `just/zephyr-{ci,dev}.just` `NROS_BUILTIN_INTERFACES_DIR`: checked and
  unaffected — both resolve to `/opt/ros/<distro>/share/` or
  `packages/cli/interfaces/`, never `packages/interfaces/`.

### Beyond the doc

**Rule 5 of `check-message-crate-identity`**: a shipped generated crate is
`nros-`prefixed. W4 asked for this as a one-time assertion; it is a gate instead,
because what makes it true is ONE `rm -rf` line in the recipe — `geometry_msgs`
arrives in this closure unrenamed, and a committed crate under that name is the
same hard cargo error rule 4 is about, one axis over. Negative control: planting
`generated/humble/geometry_msgs` makes the gate red naming that path; removing it
makes it green.

**Issue 1309** (open) gets a note: two of its nine "excluded and built by no
lane" crates were the metadata shells this phase deleted, and they were the two
it had already marked "not a defect". `check-workspace-exclude-list`'s R6 rule
named one of them as its standing example; it now names none, because an example
is the part of a derived rule that goes stale.
