---
id: 1455
title: "A shipped generated msg crate's `links` is the ament name, not the renamed
  crate name — so it collides with a consumer's own copy of the same package at
  resolve time"
status: resolved
type: bug
area: [codegen, interfaces]
severity: high
found: 2026-09-22
related: [issue-1428, rfc-0067, rfc-0023, phase-403, phase-465]
---

## What is wrong

Codegen emits `links = "nros_msgs_<ament_package>"` on every generated message
crate (`rosidl-codegen/src/bounds.rs::links_key`). It is cargo's metadata channel
and nothing else — no native library is linked; it is what makes the crate's
`build.rs` size bounds reach a dependent as `DEP_NROS_MSGS_<PKG>_BOUNDS_*`
(phase-403 W6). The emitter states the assumption that makes it safe
(`rosidl-bindgen/src/generator.rs`):

> Cargo requires `links` to be unique across a dependency graph; a generated
> crate is named after its ament package, which already is.

**The committed core set is not named after its ament package.** It is renamed
into the `nros-` namespace, and `apply_package_renames`
(`cargo-nano-ros/src/lib.rs`) rewrites `[package] name`, dependency keys,
`"../<dep>"` sibling paths and `<pkg>/std` — **not `links`**. So
`packages/interfaces/rosgraph-msgs/generated/humble/nros-builtin-interfaces-clock`
ships:

```toml
name  = "nros-builtin-interfaces-clock"
links = "nros_msgs_builtin_interfaces"     # the ament value
```

A consumer's own `nros sync` emits an **unrenamed** `builtin_interfaces` crate
carrying that same `links`. The rename bought a distinct crate name and nothing
else, so the two collide.

## Measured, 2026-09-22

A leaf depending on both a user-shape `builtin_interfaces` (the shipped `-clock`
crate with only its `[package] name` un-renamed, which is exactly the pre-rename
emitter output) and the shipped `nros-builtin-interfaces-clock`:

```
$ cargo metadata --format-version 1 --offline
error: failed to select a version for `nros-builtin-interfaces-clock`.
package `nros-builtin-interfaces-clock` links to the native library
`nros_msgs_builtin_interfaces`, but it conflicts with a previous package which
links to `nros_msgs_builtin_interfaces` as well:
package `builtin_interfaces v0.0.0 (…)`
note: only one package in the dependency graph may specify the same links value
```

**Resolve-time**, so it takes every cargo command in that leaf, not one build.

### How a consumer reaches it

`nros/sim-time` → `nros-node/sim-time` → `nros-rosgraph-msgs` →
`nros-builtin-interfaces-clock`. Any leaf that enables that public feature **and**
generates a msg closure containing `builtin_interfaces` — i.e. almost any msg
package, since `builtin_interfaces/msg/Time` is in most closures — is
unresolvable.

**Reachable, not yet reached.** `sim-time` is off by default and its two in-tree
consumers (`packages/testing/nros-tests/bins/sim-clock-{listener,publisher}`)
deliberately path-dep the committed bindings rather than generating, so no
tracked leaf hits it. Nothing prevents a user from hitting it on day one.

### It grows with every regeneration

Only two of the eight shipped crates carry `links` today — `-clock` and
`nros-rosgraph-msgs`, the phase-425 vintage. A current `nros` emits `links` for
every crate, so each regeneration of an older copy adds one more collision
surface for consumers of that ament package:

| shipped crate | `links` |
| --- | --- |
| `nros-builtin-interfaces-clock` | `nros_msgs_builtin_interfaces` |
| `nros-rosgraph-msgs` | `nros_msgs_rosgraph_msgs` |
| the other six | *(none — pre-phase-403 vintages)* |

This is the same mechanism issue 1428 measured **inside** the repo (giving the
two older `builtin_interfaces` copies the `links` line makes the whole workspace
unresolvable on `more than one crate with links=nros_msgs_builtin_interfaces`).
1428 recorded it as an in-tree regeneration hazard; it is also a consumer-facing
resolve failure, which is why it is filed separately and at higher severity.

## The fix, and why it is decidable now

**`links` follows the crate name** — `nros_msgs_` + the renamed crate's ident —
so a renamed crate and an unrenamed copy of the same ament package coexist. Either
`apply_package_renames` rewrites the `links` line alongside `name`, or the rename
is passed into the emitter so `links_key` is computed from the final crate name
(preferable: one derivation, no post-pass, and it keeps `links_key`'s doc-comment
true).

Issue 1428 left this "to be decided WITH the collapse", on the grounds that a
third option — *stop renaming `builtin_interfaces` entirely* — only makes sense
if the collapse lands. **That option is dead**, measured the same day: two `path`
packages with the same `name` + `version` are a hard error even when renamed at
the dep site and given distinct `links`:

```
error: package collision in the lockfile: packages
builtin_interfaces v0.0.0 (…/a) and builtin_interfaces v0.0.0 (…/b) are
different, but only one can be written to lockfile unambiguously
```

So the `nros-` prefix on the shipped set is permanent, the rename is permanent,
and `links` must follow it whichever way phase-465 goes. Rule recorded as
RFC-0067 §D4.

## Acceptance

A leaf that enables `nros/sim-time` **and** carries a generated
`builtin_interfaces` resolves — and that leaf is the test, because the failure is
at resolve time and no unit test can see it. A gate is possible but weaker: a
tracked generated crate whose `links` does not match its own `[package] name`.
Note that renaming `links` changes the `DEP_<LINKS>_BOUNDS_*` channel name;
**nothing in the tree reads that channel from a Rust build script today** (the
only in-tree reader of the bounds is cmake, via `nros_message_bounds.json` —
`NanoRosGenerateInterfaces.cmake`), so the convention is still free to choose.

## Resolution — 2026-09-24

`links` now follows the crate name. Reproduced first, fixed, and measured in
both directions.

### Reproduced, verbatim

A leaf path-depending on both the shipped `nros-builtin-interfaces-clock` and a
user-shape `builtin_interfaces` (that same crate with only `[package] name`
un-renamed — the pre-rename emitter output):

```
$ cargo metadata --format-version 1 --offline
error: failed to select a version for `nros-builtin-interfaces-clock`.
    ... required by package `nros-1455-repro-leaf v0.0.0 (…/leaf)`
versions that meet the requirements `*` are: 0.0.0

package `nros-builtin-interfaces-clock` links to the native library
`nros_msgs_builtin_interfaces`, but it conflicts with a previous package which
links to `nros_msgs_builtin_interfaces` as well:
package `builtin_interfaces v0.0.0 (…/user-builtin-interfaces)`
    ... which satisfies path dependency `builtin_interfaces` of package
        `nros-1455-repro-leaf v0.0.0 (…/leaf)`
note: only one package in the dependency graph may specify the same links value
...
help: try to adjust your dependencies so that only one package uses the
      `links = "nros_msgs_builtin_interfaces"` value
```

Exit 101, at `cargo metadata` — before any build, as filed.

### Where the fix went, and why not the other place

In **`apply_package_renames`** (`cargo-nano-ros/src/lib.rs`), as a sixth thing
the rename pass rewrites, not in the emitter.

The issue proposed the emitter as preferable ("one derivation, no post-pass").
On reading the code the argument inverts. `apply_package_renames` is the ONE
place that knows what a generated crate is finally called: it owns the directory
name, the `[package] name`, every sibling dep key, every `<pkg>/std` feature
reference and every `use` path in the sources. Teaching the emitter the rename
map would not remove the post-pass — the other five rewrites still need it — it
would only split "what does this crate ship as?" across two functions, which is
the shape CLAUDE.md warns about. The emitter also writes into
`output_dir/<ament package>/`, which the rename pass then moves, so it would be
emitting a manifest whose `links` and whose directory disagreed.

The FORMULA stays single either way, and that is the part that mattered: the
value is **recomputed** through the emitter's own
`rosidl_codegen::BoundInventory::links_key`, never text-substituted. A textual
`old → new` swap would have been a second spelling and would also have been
wrong — `links_key` normalises `-`/`.`/`/` to `_`, so for any rename whose old
name is not spelled the way the key spells it the substitution finds nothing and
silently leaves the ament value in place.

`links_key`'s parameter is renamed `package → crate_name` and both its
doc-comment and the emitter's comment — the two places that stated the
assumption this issue disproved — now say what the value is a function of.

`rewrite_package_links` rewrites, and never ADDS: a crate with no `links` key
stays without one. The six pre-phase-403 shipped crates emit no bounds
`build.rs`, so a `links` key there would claim a graph-global name while writing
nothing to the channel — strictly worse than not claiming it. The rewrite is
also scoped to the `[package]` table, since a dependency may legitimately be
named `links`.

### What changed on disk, and how

Both crates that carry `links` are in `packages/interfaces/rosgraph-msgs/`:

| crate | before | after |
| --- | --- | --- |
| `nros-builtin-interfaces-clock` | `nros_msgs_builtin_interfaces` | `nros_msgs_nros_builtin_interfaces_clock` |
| `nros-rosgraph-msgs` | `nros_msgs_rosgraph_msgs` | `nros_msgs_nros_rosgraph_msgs` |

`just generate-rosgraph-msgs` was run first, against this branch's CLI, and it
produced exactly those two values. It also dragged in five codegen versions of
unrelated drift — `NROS_EMITTED_CODEGEN_VERSION` 2 → 7, a new
`cyclone_schema_shape` block in both bounds artifacts, `ament_version`
1.2.1 → 1.2.2 on `nros-rosgraph-msgs`, and unformatted sources (the committed
tree has been through `cargo fmt`). That is a separate decision about how stale
the shipped vintage is allowed to be, so the regeneration was reverted and the
two `links` lines were applied **by hand**; the regenerated manifests are the
evidence that hand and recipe agree, line for line:

```
nros-builtin-interfaces-clock AGREE: links = "nros_msgs_nros_builtin_interfaces_clock"
nros-rosgraph-msgs            AGREE: links = "nros_msgs_nros_rosgraph_msgs"
```

The next full regeneration of that tree re-derives the same two values and
carries the rest of the drift with it.

### The consumer side of the channel — verified independently

Renaming the channel renames `DEP_NROS_MSGS_<PKG>_BOUNDS_*`, and a `build.rs`
reading a missing env var gets `None` rather than an error, so this had to be
measured rather than assumed. Issue 1428's finding holds:

* **Rust**: every `DEP_*` read in the tree, over all `build.rs` files, is
  `DEP_DDSC_INCLUDE` / `DEP_DDSC_IDLC` (`nros-rmw-cyclonedds-sys`, off
  cyclonedds-sys's `links = "ddsc"`) and `DEP_NROS_NODE_*` (`nros-c`, off
  `nros-node`'s `links = "nros_node"`). **No reader of `DEP_NROS_MSGS_*`
  exists** — the string appears only in comments, this issue, RFC-0067,
  RFC-0087, phase-403 and archived issue 0963.
* **cmake**: the bounds reach cmake as a FILE, not through cargo's env channel —
  `nros_message_bounds_files()` (`NanoRosCodegenCore.cmake`) hands
  `<output_dir>/nros_message_bounds.{json,cmake}` to
  `NanoRosGenerateInterfaces.cmake`, which registers the `.cmake` projection as
  a fragment. Nothing there spells a `links` key.
* The literal `nros_msgs_` appears in exactly two non-doc places besides the two
  manifests: `links_key` itself and one `rosidl-bindgen` unit test.

So the convention was free to change, and the only in-tree consequence is the
two manifest lines.

### Acceptance, both directions

**Positive** — the reproduction resolves:

```
$ cargo metadata --format-version 1 --offline
     Locking 9 packages to latest compatible versions
rc=0
```

`nros-builtin-interfaces-clock` (`nros_msgs_nros_builtin_interfaces_clock`) and
`builtin_interfaces` (`nros_msgs_builtin_interfaces`) now coexist.

**Negative** — a collision that is REAL is still reported. The same ament
package generated into two output trees under the SAME final crate name is the
case the `nros-` prefix exists to prevent, and it must still fail:

```
package `nros-builtin-interfaces-clock` links to the native library
`nros_msgs_nros_builtin_interfaces_clock`, but it conflicts with a previous
package which links to `nros_msgs_nros_builtin_interfaces_clock` as well:
package `nros-builtin-interfaces-clock v0.0.0 (…/tree-a)`
```

Exit 101. The fix makes distinct crates distinct; it does not make everything
unique by construction.

### The gate

Rule 4 of `check-message-crate-identity` (`just check message-crate-identity`,
buildless, fast lane), which is issue 1428's gate for exactly this family —
"which crate *is* `builtin_interfaces`?" — so the rule joins it rather than
becoming a fourteenth script. A tracked generated crate that declares `links`
must declare `nros_msgs_` + its own `[package] name`; declaring none stays
legal.

Deliberately stated as a property of the crate's OWN name rather than of a
rename map, because that is what a reader of the shipped file can check without
knowing which recipe produced it. It was red on both crates before the fix and
names the required value:

```
generated message crate(s) whose `links` does not follow their name:
  nros-builtin-interfaces-clock: links = 'nros_msgs_builtin_interfaces',
      must be 'nros_msgs_nros_builtin_interfaces_clock'    packages/…/Cargo.toml
  nros-rosgraph-msgs: links = 'nros_msgs_rosgraph_msgs',
      must be 'nros_msgs_nros_rosgraph_msgs'    packages/…/Cargo.toml
```

and green after, reporting its own reach so it cannot go vacuous unnoticed:

```
check-message-crate-identity: OK (8 generated crate(s), 2 declaring `links`,
328 manifest(s), 6 baselined duplicate claim(s))
```

Its selftest — which runs on the normal path — carries the formula and both
directions: a renamed crate and a consumer's copy must differ, two copies
shipping under one name must not.

### What this does NOT do

It does not touch the `builtin_interfaces` triplication (RFC-0067 §D5,
phase-465), and does not depend on it: §D4 is decidable alone, because dropping
the `nros-` prefix was measured dead the same day this was filed. It also does
not give the six `links`-less shipped crates a key — only a regeneration will,
and it will derive the right one.

Rule recorded as RFC-0067 §D4 (**Landed** note added there), reader-facing
summary in `packages/interfaces/README.md`.
