---
id: 1428
title: "`builtin_interfaces` is generated three times in the core pre-generated
  set and none of the three is canonical — so nothing can depend on it by name"
status: resolved
type: tech-debt
area: [codegen, interfaces]
severity: medium
found: 2026-09-21
related: [phase-417, phase-425, phase-465, rfc-0023, rfc-0067, issue-0394, issue-1455]
---

## What is there

Three crates, one upstream package:

| path | crate | first emitted |
| --- | --- | --- |
| `packages/interfaces/rcl-interfaces/generated/humble/nros-builtin-interfaces` | `nros-builtin-interfaces` | `23e12afec` (edition-aware binding generation) |
| `packages/interfaces/diagnostic-msgs/generated/humble/nros-builtin-interfaces-diag` | `nros-builtin-interfaces-diag` | `1916314ec` (phase-296 W3b.1) |
| `packages/interfaces/rosgraph-msgs/generated/humble/nros-builtin-interfaces-clock` | `nros-builtin-interfaces-clock` | `caa546fb2` (phase-425 W2) |

All three are root workspace members (`Cargo.toml:131,136,142`).

**Their Rust sources are byte-identical.** Measured 2026-09-21 — `src/lib.rs`,
`src/msg/mod.rs`, `src/msg/time.rs`, `src/msg/duration.rs`, each with one md5
across all three (`924b3f2d…`, `c6c0bcb8…`, `f386bd08…`, `15a1c8e1…`). All three
declare the same wire identity, `TYPE_NAME = "builtin_interfaces/msg/Time"`
(`src/msg/time.rs:51`) — so they are one type on the wire and three types in
Rust, with no conversion between them, and `nros-tests` already path-deps two of
them at once (`packages/testing/nros-tests/Cargo.toml:551,556`).

## Why three, and why it is not an accident

Codegen emits the **whole transitive interface closure into one flat directory**,
wiring siblings by `path = "../<dep>"`
(`packages/cli/rosidl-bindgen/src/generator.rs:224,811-819`;
`packages/cli/cargo-nano-ros/src/lib.rs:350-402`). `rcl_interfaces`,
`diagnostic_msgs` and `rosgraph_msgs` each reference `builtin_interfaces/msg/Time`,
so each closure contains its own copy. There is **no reuse map, no canonical-crate
table and no "already have this package" skip** across trees: the only dedupe is a
per-invocation `emitted: HashSet<String>` inside ONE output tree
(`packages/cli/nros-cli-core/src/cmd/ws.rs:2980-2982,3384`).

The suffix is not a codegen decision either. Codegen names a crate after its ament
package, verbatim; `nros-` and `-diag` / `-clock` are `--rename old=new` flags in
the repo `justfile` (`:4275-4276`, `:4291-4293`, `:4312-4313`), applied as a
post-generation textual rewrite of directory names, `[package] name`, dependency
keys, `"../<dep>"` paths and `<pkg>/std` (`cargo-nano-ros/src/lib.rs:453-529`).
The `justfile` already states the reason in as many words (`:4297-4301`): *"a
generated crate names its deps by CRATE name, so two trees generating
`builtin_interfaces` collide in one workspace"*.

So the suffix is a workaround that lets three closures coexist. It works. What it
does not do is produce a canonical crate.

## What the three differ in, beyond their names

The sources are identical; the **manifests are three different codegen vintages**,
and one of them carries a build product the other two do not.

* **`nros-builtin-interfaces`** (oldest) is a hand-adapted workspace member:
  `version.workspace = true`, `edition.workspace = true`, `license.workspace`,
  `repository.workspace`, a `description`, a `[package.metadata.ros]` block
  (`upstream-package` / `upstream-version` / `ros-edition`) and
  `.workspace = true` dependency rows. **This is not a shape codegen emits.**
  Its one consumer names it with a VERSION —
  `nros-builtin-interfaces = { version = "0.5.0", path = "../nros-builtin-interfaces" }`
  (`nros-rcl-interfaces/Cargo.toml:27`) — which is the spelling issue 0394 and
  CLAUDE.md both say a generated message crate must not carry.
* **`nros-builtin-interfaces-diag`** is the middle vintage: `version = "0.0.0"`,
  `edition = "2021"`, `[package.metadata.nros] ament_version`, relative path deps,
  and the phase-359 W10 comment explaining the removed `std` feature. No `links`,
  no `build.rs`.
* **`nros-builtin-interfaces-clock`** is what codegen emits today: the same
  standalone manifest **plus `links = "nros_msgs_builtin_interfaces"`**, plus a
  `build.rs` that publishes the phase-380 size bounds on the `DEP_<LINKS>_BOUNDS_*`
  channel, plus `nros_message_bounds.json`. It is the only one of the three with
  either file.

## The live consequence: the three copies cannot all be regenerated

`links` is derived from the **ament** package name
(`packages/cli/rosidl-codegen/src/bounds.rs:826` —
`format!("nros_msgs_{}", …)`), and the `--rename` post-pass rewrites `name`,
dependency keys, `"../<dep>"` and `<pkg>/std` and **does not touch `links`**
(`cargo-nano-ros/src/lib.rs:494-517`). The emitter's own comment states the
assumption that makes this safe and that the rename breaks
(`rosidl-bindgen/src/generator.rs:763-764`): *"Cargo requires `links` to be unique
across a dependency graph; a generated crate is named after its ament package,
which already is."* After a rename it is not.

MEASURED, not read. Giving the two older copies the `links` line and the `build.rs`
a current `nros` would emit — i.e. simulating the then-separate `rcl-interfaces`
and `diagnostic-msgs` regeneration recipes, which phase-465 W1 replaced with the
single `just generate-interfaces` — makes the **whole workspace unresolvable**:

```
$ cargo metadata --format-version 1 --offline
error: Attempting to resolve a dependency with more than one crate with links=nros_msgs_builtin_interfaces.
This will not build as is. Consider rebuilding the .lock file.
```

(Probe reverted; the tree is unchanged.) That is a resolve-time failure, so it
takes every cargo command in the workspace, not one consumer.

The tree already pays for this, and says so. `4b80db633`'s message, regenerating
the eight committed msg crates for a codegen-version bump: *"only `src/lib.rs`
copied back, because their `Cargo.toml`s are hand-maintained in three shapes an
in-place generate would clobber."* So the documented procedure for updating these
crates is to generate out of tree and copy one file back per crate — which is how
three manifests stay three vintages, and why `nros-builtin-interfaces` and
`-diag` have never received the `links` + `build.rs` pair the bounds channel
needs.

## Why this blocks `rust:Time::to_ros_msg`

phase-417's G8 build list carries the row as *"blocked on a DECISION, not on a
body: does `builtin-interfaces` join the pre-generated core set?"* **The premise
is wrong — it joined three times.** `packages/interfaces/*` IS the pre-generated
core set (CLAUDE.md's named exception to the "never commit `generated/`" rule),
and `builtin_interfaces` has been in it since `23e12afec`.

The real blocker is that there are three and none is canonical, so there is no
crate name a consumer can depend on and get "the" `builtin_interfaces::msg::Time`.
Picking one today means picking a suffix that names another package's tree.

The row's second clause is correct and is a separate constraint: `nros-core`
depends on `nros-serdes`, `log` and `heapless` and nothing else
(`packages/core/nros-core/Cargo.toml:41-44`), while every generated message crate
depends on `nros-core` — so `nros-core` cannot depend on a message crate without a
cycle, and `nros_core::Time::to_ros_msg` cannot be an inherent method whatever the
canonical crate turns out to be. The conversion has to live in the message crate
(which owns one of the two types, so `impl From<nros_core::Time> for Time` is
legal there) or above both. That choice is still open; it just is not what is
blocking.

## Is collapsing a file move or a codegen change? — CODEGEN

A file move cannot do it, for three reasons, each measured above:

1. **The dependency spelling is emitted.** `nros-rcl-interfaces`,
   `nros-std-msgs-diag` and `nros-rosgraph-msgs` each name their `builtin_interfaces`
   dep as a sibling `path = "../<name>"` row that codegen writes
   (`generator.rs:811-819`). Moving the crate out of the tree makes those rows
   wrong, and the next regeneration writes them back.
2. **The suffix is what makes coexistence legal.** Deleting two copies and
   pointing the three parents at one requires the parents to name a crate that is
   not their sibling — which codegen has no way to express. Adding it needs a
   package→(crate name, path) map consulted at `generator.rs:811-819`, and a way to
   skip the dep in `filter_interface_packages` / `codegen_ament_deps_for`.
3. **`links` has to move with it.** Whichever crate survives must be the one with
   `links` + `build.rs`, and the rename pass must either stop renaming
   `builtin_interfaces` or start rewriting `links` — otherwise the collapse
   re-creates the collision measured above the first time anyone regenerates.

So: a file move plus three manifest edits would produce a tree that is correct
until the next `just generate-*`, which is the shape CLAUDE.md warns about for
anything under `generated/`. The fix belongs in `packages/cli/`.

## What is NOT claimed here

Nothing is broken today. The three copies resolve, build and ship; the wire
identity is the same in all three, so no message is mis-serialised. The costs are
(a) a blocked build-list row, (b) three manifests that cannot be regenerated in
place, (c) two crates permanently missing the size-bounds channel their sibling
has, and (d) two Rust types for one wire type in any consumer that reaches both.
This is tech-debt with a named blocker, not a bug report.

## Direction, not decided

The cheapest honest first step is to stop the bleeding rather than to collapse:
make the rename pass rewrite `links` (or refuse a rename that would collide), so
the three copies become regenerable and the next `just generate-*` cannot wedge
the workspace. The collapse itself needs the reuse map in (2) above, and that is
an RFC-sized decision about whether a generated tree may reference a crate outside
itself — which is the same question `nros sync`'s central `[patch.crates-io]` file
answers for a different set of crates.

## What landed — 2026-09-21 (the triplication is NOT collapsed; read on)

Every measurement above was re-verified before anything was touched, and all of
them held: the three paths; the four byte-identical sources (md5 `924b3f2d…`,
`15a1c8e1…`, `c6c0bcb8…`, `f386bd08…`, one value each across all three crates);
the shared `TYPE_NAME = "builtin_interfaces/msg/Time"`; the `links` +
`build.rs` + `nros_message_bounds.json` on `-clock` alone; the version-carrying
dep row in `nros-rcl-interfaces`; and `nros-tests` path-depping two copies at
once.

### The shape chosen: leave three, fix the version spelling, gate a fourth

Not collapsed, deliberately. The issue's own section three is right that this is
a codegen change and not a file move, and there is a fourth argument for leaving
it that the section does not make:

**a canonical crate would make generated trees non-relocatable.** Today every
closure is flat and self-contained, so a `generated/` tree can be copied,
regenerated or moved with no reference outside itself. Pointing the three
parents at one shared crate means a generated manifest naming a path outside its
own tree — and for an out-of-tree consumer that path leads into the nano-ros
checkout. That collides head-on with "Examples are standalone copy-out projects;
no workspace walk-up" (CLAUDE.md, RFC-0026), and it is the same question
`nros sync`'s central `[patch.crates-io]` file answers for a different set of
crates, with a different answer. So the collapse is not merely expensive; it
trades a property the tree currently has. That is an RFC-sized decision and it
stays open here rather than being pre-empted by a file move.

Against "leave three and say nothing", which is the status quo: one of the three
carried a spelling CLAUDE.md forbids, and nothing in the tree could notice a
fourth copy. Both are fixed below.

### The version spelling — the class was FIVE rows, not one

The issue named `nros-rcl-interfaces/Cargo.toml:27`. A sweep of every tracked
manifest found the same defect in four more places, and a second half the issue
did not name:

* **three generated crates carried `version.workspace = true`** — the *release*
  version — rather than the `0.0.0` constant: `nros-builtin-interfaces`,
  `nros-rcl-interfaces` and **`nros-lifecycle-msgs`**, which is in neither of the
  issue's tables but is the same hand-adapted vintage.
* **five dep rows pinned it**: `nros-rcl-interfaces/Cargo.toml:27`,
  `packages/core/nros-node/Cargo.toml:231,232` and
  `packages/rmw/cyclonedds/nros-rmw-cyclonedds/Cargo.toml:95,96`.

This is not only a style rule. A path dep's `version` is still a REQUIREMENT:
the workspace version is `0.5.0` and all five rows read `version = "0.5.0"`, so
`^0.5.0` stops matching the day the workspace bumps to `0.6.0` — a resolve-time
failure, which takes every cargo command in the tree rather than one consumer.
The tree was one version bump away from that.

All eight sites are now the constant `0.0.0` with `path` alone, which is also
exactly what codegen emits (`rosidl-bindgen/src/generator.rs:768`, asserted at
`:1487`) — so the three hand-adapted manifests moved *toward* the emitted shape
on this field rather than away from it. Lock impact, via `just lock-update`
only: three lines in the root `Cargo.lock`, two in
`bins/sim-clock-listener/Cargo.lock` (which is tracked and was NOT in the
drift baseline, so it had to move with them). Nothing else re-resolved.

Out-of-tree consumers are untouched: codegen already emitted `0.0.0`, so no
user's `nros sync` closure ever had this shape.

### The gate — `check-message-crate-identity`

`scripts/check-message-crate-identity.py`, buildless, on the derived fast lane
(`just check message-crate-identity`). Three rules:

1. a tracked generated message crate's `version` is the constant `0.0.0`;
2. no dep row, in **any** tracked manifest, pins one of their versions;
3. no wire `TYPE_NAME` is claimed by more than one shipped crate.

Rule 3 is a shrink-only ratchet, `.config/duplicate-wire-type-baseline.txt`,
holding exactly the six claims of the `builtin_interfaces` triple
(`Time` and `Duration` × three crates). A new duplicate fails; a baselined one
that stops duplicating fails as *stale*, so the debt cannot silently go hollow
(the issue-0743 class). **This is what makes a fourth copy impossible to land
unnoticed**, which is how the third arrived.

Reach, per CLAUDE.md's 0196 rule: rules 1–2 read all **328** tracked manifests
across every workspace root, not the interfaces tree — which is what found the
four extra sites. Rule 3 reads every tracked `.rs`.

`#[cfg(test)]` claims are excluded, and that exclusion is load-bearing. Measured:
**four** type names are claimed by more than one crate, but two
(`std_msgs/msg/Header`, `std_msgs/msg/Int32`) and three extra claimants
(`nros-serdes` on `builtin_interfaces/msg/Time`, `nros-rmw-cyclonedds`) are
hand-written fixture structs inside `#[cfg(test)] mod tests`. A fixture is never
linked into an image, so it cannot collide on the wire; counting them would have
put two non-problems in the baseline beside the real one. The shipped duplicate
population is exactly the triple.

Five planted violations, each confirmed red and reverted: a version-carrying dep
row; a generated crate off `0.0.0`; a fourth crate claiming
`std_msgs/msg/Header`; a baselined duplicate removed (stale); and the control —
a `#[cfg(test)]` fixture claiming a duplicate, which must stay GREEN and does.
The script also self-tests its Rust scanner (comments, raw strings, lifetimes
vs. char literals, nested `cfg(test)`) on every run, per phase-395.

## What is deliberately LEFT, and why

**The collapse itself.** Phase-sized, and argued above: it needs the
package→(crate name, path) reuse map at `generator.rs:811-819`, a skip in
`filter_interface_packages` / `codegen_ament_deps_for`, and a decision on
whether a generated tree may reference a crate outside itself — which costs the
relocatability property. Not started.

**The `links` rename hazard** (this issue's "Direction, not decided"). Still
live: `apply_package_renames` (`cargo-nano-ros/src/lib.rs:453-529`) rewrites
`name`, dep keys, `"../<dep>"` and `<pkg>/std`, and does **not** touch `links`,
so giving the two older copies the `links` + `build.rs` pair a current `nros`
emits still wedges the workspace on `more than one crate with
links=nros_msgs_builtin_interfaces`. Left on purpose, with a new measurement
that bears on how to fix it: **nothing in the tree reads
`DEP_<LINKS>_BOUNDS_*` from a Rust build script today** — the only in-tree
reader of the bounds is cmake, via the JSON file
(`NanoRosGenerateInterfaces.cmake:390`). So the channel-naming convention is
still free to choose, and choosing it now (rewrite `links` to follow the new
crate name? refuse the rename? stop renaming `builtin_interfaces` entirely?)
would pre-empt the collapse decision, since the third option only makes sense
if the collapse lands. It should be decided WITH the collapse, not before it.

**Two Rust types for one wire type** in any consumer reaching both: unchanged.
`nros-tests` still path-deps two copies.

So: the bleeding this issue names is not stopped — a regeneration still wedges —
but the tree is no longer one version bump from an unresolvable workspace, and a
fourth copy can no longer arrive unnoticed. Status stays **open** for the
collapse and the `links` decision.

## Decision — 2026-09-22: collapse, but not to a canonical crate

State re-verified first, and nothing had moved: three crates present; the four
sources byte-identical across all three (md5 `924b3f2d…`, `c6c0bcb8…`,
`f386bd08…`, `15a1c8e1…`, one value each); `links` + `build.rs` +
`nros_message_bounds.json` on `-clock` alone; all eight version sites the
constant `0.0.0` with no dep row pinning one; `check-message-crate-identity`
green over 8 generated crates / 328 manifests / 6 baselined claims; `nros-tests`
still path-depping two copies.

**The collapse should happen. The shape everyone assumed is the wrong one, and
the shape that is right needs no codegen change and costs no property.**

Both this issue's §"Is collapsing a file move or a codegen change? — CODEGEN"
and the Resolution above reason about **a shared canonical crate reached from
four trees**. Given four trees they are correct: the parents' sibling `path` rows
are emitted, so a file move is undone by the next `just generate-*`; and a
manifest naming a path outside its own tree is a real loss, which for an
out-of-tree consumer leads into the nano-ros checkout.

The alternative is to make the **tree** canonical rather than the crate. Codegen
already deduplicates inside one invocation — `resolve_transitive_dependencies`
returns a `HashSet`, `filter_interface_packages` iterates it — so one driver
`package.xml` depending on all four core packages emits each ament package
exactly once. MEASURED 2026-09-22 (parent checkout's release `nros`, this
worktree having none):

```
Generating bindings for 7 interface packages...
  ✓ builtin_interfaces (2 messages, …)   ← once, not three times
  ✓ std_msgs (30 messages, …)            ← once
nros-rosgraph-msgs:   nros-builtin-interfaces = { path = "../nros-builtin-interfaces" }
nros-diagnostic-msgs: nros-std-msgs           = { path = "../nros-std-msgs" }
```

Six crates where four trees need eight; every dep still a flat sibling; every
crate `nros-`prefixed; `links` unique because there is one copy; sources matching
the committed ones modulo `cargo fmt`. So:

* **"it is a codegen change"** — no. The emitted rows are already right. It is a
  driver-package, recipe and layout change.
* **"it costs relocatability"** — no. State the property precisely as *no
  generated tree references another generated tree*, and one tree preserves it
  trivially. (The looser reading, *no reference outside itself*, was never true:
  every generated crate already reaches `nros-core` / `nros-serdes` in the
  checkout by relative path, by RFC-0067's own design.)
* **an out-of-tree consumer whose own closure contains `builtin_interfaces`** is
  unaffected either way — `nros sync` already emits one copy per ament package
  for the whole workspace closure, before and after. What changes is only how
  many copies nano-ros ships.

Recorded as **RFC-0067 §D5** (the rule: one wire type, one crate per
*(ros-edition, capacity profile)*; two copies are legitimate only when they
differ). Planned as **phase-465**. Reader-facing summary where someone who finds
three identical crates will look: **`packages/interfaces/README.md`**.

**Affordable now?** No, and it is filed rather than started. The migration is a
~25-file rename — root member list, ~19 consumer dep rows whose KEYS change with
the `-diag`/`-clock` names, four recipes into one, three hand-adapted manifests
reshaped, the tracked lockfiles, the gate baseline emptied, and a
`schema_serializer_round_trip` corpus that asserts its row count against what
`packages/interfaces` defines — and its only honest acceptance is a build. A
half-applied version of exactly this rename is the issue-0394 class that broke a
fresh clone twice.

### The `links` decision is NOT blocked by the collapse

The Resolution above left it to "be decided WITH the collapse", because the third
option — *stop renaming `builtin_interfaces` entirely* — only makes sense if the
collapse lands. **That option is dead**, measured: two `path` packages with the
same `name` + `version` are a hard error even when renamed at the dep site and
given distinct `links` —

> package collision in the lockfile: packages `builtin_interfaces v0.0.0 (…/a)`
> and `builtin_interfaces v0.0.0 (…/b)` are different, but only one can be
> written to lockfile unambiguously

— so the `nros-` prefix on the shipped set is permanent and `links` must follow
the crate name whichever way phase-465 goes. Which also makes the hazard worse
than this issue recorded: it is not only an in-tree regeneration wedge but a
**consumer-facing resolve failure**, reachable today via `nros/sim-time` →
`nros-node/sim-time` → `nros-rosgraph-msgs` → `nros-builtin-interfaces-clock` in
any leaf that also generates a closure containing `builtin_interfaces`:

> package `nros-builtin-interfaces-clock` links to the native library
> `nros_msgs_builtin_interfaces`, but it conflicts with a previous package which
> links to `nros_msgs_builtin_interfaces` as well

Split out as **issue 1455** (severity high, independent of phase-465), rule as
**RFC-0067 §D4** — and **resolved 2026-09-24**: `apply_package_renames`
recomputes `links` from the final crate name through `links_key`, the two
shipped crates were corrected on disk, and it is rule 4 of this issue's own
gate. 1428's measurement that giving the two older `builtin_interfaces` copies a
`links` line makes the workspace unresolvable no longer holds — each would get
a key derived from its own renamed name.

### Status

Stays **open**: the three crates are still in the tree. Closed by phase-465;
nothing further is open on this issue's own analysis.

## RESOLVED — 2026-09-25, phase-465

The collapse landed exactly as RFC-0067 §D5 and the phase doc specified: **one
driver package, one output tree, six crates**. `packages/interfaces/` now holds
one `package.xml` with four `<depend>` rows, one `nros-codegen.toml`, and
`generated/humble/{nros-builtin-interfaces, nros-std-msgs, nros-rcl-interfaces,
nros-diagnostic-msgs, nros-rosgraph-msgs, nros-lifecycle-msgs}`. The three
`builtin_interfaces` crates are ONE crate; `nros-std-msgs-diag` is
`nros-std-msgs`; `nros-builtin-interfaces-diag` and
`nros-builtin-interfaces-clock` are gone.

**No codegen change was required**, as §D5 predicted and this run re-measured
with the branch's own CLI: one invocation emitted 7 packages with
`builtin_interfaces` and `std_msgs` appearing once each, every dep still a flat
`path = "../<dep>"` sibling, every crate `nros-`prefixed.

**The proof is the gate, not the claim.** Rule 3's baseline is a shrink-only
ratchet, so with the tree collapsed `check-message-crate-identity` reported all
six baselined claims as STALE — a tree that had only *looked* collapsed would
have left rows behind. `.config/duplicate-wire-type-baseline.txt` is empty, and
the gate reads: 6 generated crates, 6 declaring `links`, 324 manifests, 0
duplicate wire-type claims, 0 version-carrying dep rows, 0 crates off `0.0.0`, 0
`links` values that do not follow their name.

**The acceptance was a build, twice.** `just generate-interfaces` run two times
in a row leaves the worktree clean — in-place idempotence, which four trees never
had. That is what retires this issue's "the documented procedure for updating
these crates is to generate out of tree and copy one file back per crate": the
six manifests are now exactly what codegen emits, so there is nothing
hand-maintained left to clobber.

### What the regeneration cost, decided rather than absorbed

Taking the current codegen vintage was not optional — a tree that differs from
what the recipe emits is a tree the second run rewrites, so pinning the old
vintage and claiming idempotence are mutually exclusive. It was also cheap:
`NROS_EMITTED_CODEGEN_VERSION` 2 -> 7 (the runtime's own constant is 7 and its
floor is 1), a `links` + `build.rs` + `nros_message_bounds.json` bounds channel
for the four crates that lacked one, `cyclone_schema_shape` added to the bounds
JSON, `ament_version` 1.2.1 -> 1.2.2 on this host's ROS, and a **mojibake fix** —
three `nros-diagnostic-msgs` sources carried UTF-8-read-as-latin-1 comment
banners. After `rustfmt`, 4 of 60 source files differed for any reason other than
the version constant and the renamed sibling crates.

### The three consequences this issue named

* (a) *a blocked build-list row* — phase-417's `rust:Time::to_ros_msg` is
  unblocked on its stated blocker: there is now one canonical crate name,
  `nros-builtin-interfaces`. The second constraint the issue recorded is
  untouched and still correct — `nros-core` cannot depend on a message crate
  without a cycle, so the conversion has to live in the message crate or above
  both. Not decided here.
* (b) *three manifests that cannot be regenerated in place* — fixed, and proven
  by running the recipe twice.
* (c) *two crates permanently missing the size-bounds channel* — fixed; all six
  declare `links` and emit `nros_message_bounds.json`.
* (d) *two Rust types for one wire type in any consumer that reaches both* —
  fixed. `nros-tests` path-depped `nros-builtin-interfaces` and
  `nros-builtin-interfaces-diag` at once; it has six message rows now, not seven.

### Found on the way, and worth knowing

`corpus_is_exhaustive` in `schema_serializer_round_trip.rs` counts every `impl
::nros_serdes::Message` under `packages/interfaces` and asserts the corpus
matches. phase-425 W2 added the rosgraph tree (3 impls) and no corpus row, so it
has read 76 against 79 since — `rosgraph_msgs/msg/Clock` was never round-tripped
by the sweep that exists to round-trip everything. Fixed with a
`nros-rosgraph-msgs` dev-dep and one `entry!`; it is 75 against 75.

`check-generated-schema-coverage` matched NOTHING after the move (its pathspec
required a driver-package component before `generated/`) and said so through its
own empty-list precondition rather than passing over zero files. Two more
lookups were found dead *before* the collapse and removed or fixed: the cmake
layer-3 `packages/interfaces/<pkg>` rung, and `cmd/build.rs`'s enumerator, which
stripped an `nros-` prefix from four hyphenated driver-package directory names.

### Status

**Resolved.** The `links` half was split out as issue 1455 and resolved
2026-09-24, independently and ahead of this. Nothing is open on this issue.
