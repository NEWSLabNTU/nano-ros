---
id: 1428
title: "`builtin_interfaces` is generated three times in the core pre-generated
  set and none of the three is canonical — so nothing can depend on it by name"
status: open
type: tech-debt
area: [codegen, interfaces]
severity: medium
found: 2026-09-21
related: [phase-417, phase-425, rfc-0023, rfc-0067, issue-0394]
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
a current `nros` would emit — i.e. simulating `just generate-rcl-interfaces` and
`just generate-diagnostic-msgs` — makes the **whole workspace unresolvable**:

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
