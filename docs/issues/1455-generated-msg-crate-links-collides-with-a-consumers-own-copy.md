---
id: 1455
title: "A shipped generated msg crate's `links` is the ament name, not the renamed
  crate name — so it collides with a consumer's own copy of the same package at
  resolve time"
status: open
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
