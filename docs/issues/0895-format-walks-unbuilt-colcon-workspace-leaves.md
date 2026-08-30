---
id: 895
title: "`just format` is red or green depending on whether a migrated colcon workspace has been BUILT"
status: open
area: build
severity: medium
found: 2026-08-29
related: [0359, 0463, phase-383, RFC-0065]
---

# `just format` walks into leaves whose workspace root is a build artifact

## What was measured

On a tree where `examples/workspaces/launch` has not been built, `just format`
fails:

```
cd examples/workspaces/launch/src/listener_pkg && cargo +nightly-2026-04-11 fmt
`cargo metadata` exited with an error: error: current package believes it's in a
workspace when it's not:
current:   .../examples/workspaces/launch/src/listener_pkg/Cargo.toml
workspace: .../nano-ros/Cargo.toml
```

24 leaves format successfully before it, so the failure reads as a problem with
that leaf. It is not. `examples/workspaces/launch` is a MIGRATED colcon
workspace (RFC-0065 D3 / phase-383 W10.a): `nros build` GENERATES its root
`Cargo.toml`, and `examples/workspaces/launch/.gitignore:13` hides it. The
tracked marker is `.colcon_workspace`. So the two leaves are members of a
workspace root that exists only after a build — and `just format` steps into
them regardless.

The same fact is already written down one lane over.
`nros-tests/tests/example_shape.rs::every_standalone_rust_leaf_is_its_own_workspace_root`
carries this comment:

> Reading only the manifest made this test depend on whether the workspace
> happened to have been BUILT — green on a machine that had, red on a fresh
> clone, and red here for `examples/workspaces/launch` once its root was
> deleted.

That test was taught to read `.colcon_workspace` as the tracked half of the
fact. `just format` was not, and neither was anything else that runs a bare
cargo command per leaf.

## Why it is not simply "run `nros build` first"

The state is build-dependent per WORKSPACE, so the tree is half-resolved at any
given moment. Measured on this checkout:

| workspace | root manifest on disk | rust leaves | `just format` |
| --- | --- | ---: | --- |
| `features` | yes | 10 | green |
| `realtime-rust` | yes | 3 | green |
| `rust` | yes | 9 | green |
| `safety` | yes | 3 | green |
| `sizing` | yes | 1 | green |
| `mixed` | **NO** | 1 | green — its leaf carries its own `[workspace]` |
| `launch` | **NO** | 2 | **red** |

`mixed` is the interesting row: it is equally unbuilt and equally a migrated
workspace, and it passes only because its single leaf
(`src/rust_heartbeat_pkg`) happens to carry an empty `[workspace]` table while
`launch`'s two do not. Two leaf shapes coexist inside the same workspace class,
and which one a workspace got decides whether an unbuilt tree formats.

## Scope of the class

A full sweep of tracked manifests — nearest enclosing `[workspace]`, root
`members` globs expanded, root `exclude` honoured — finds exactly two leaves
that resolve to the ROOT workspace while being neither member nor excluded:

```
examples/workspaces/launch/src/listener_pkg
examples/workspaces/launch/src/talker_pkg
```

Both confirmed against cargo itself (`cargo metadata --no-deps` in each leaf).
This is the same shape as issue 0359 / phase-320 W1.b, whose fix note still
sits in the root `Cargo.toml`:

> these two are neither members nor excluded, so cargo greets anyone who runs a
> command inside them with "current package believes it's in a workspace when
> it's not". They are unbuilt porting templates, so nothing surfaced it until
> now.

Third time for the class, second cause: 0359 was two crates missing from
`exclude`; this is two leaves whose owning root is generated.

## What a fix has to decide

Not "add two `exclude` lines" — that would make the leaves permanently
un-adoptable by the generated root they legitimately belong to. The real
question is which of two shapes a migrated workspace's Rust leaf takes, and
then making it uniform:

1. **Leaf carries its own `[workspace]`** (the `mixed` shape). Works with no
   build; costs the leaf its membership in the generated root, so
   feature unification and a shared lock are gone.
2. **Leaf is a plain member** (the `launch` shape) and every per-leaf cargo
   recipe learns the `.colcon_workspace` precondition — the same tracked marker
   `example_shape.rs` already reads, resolved the same order
   `detect_workspace_root` uses.

(2) is the one that matches what the workspace IS, and it wants a guard, not a
skip: `just check tier-preconditions` is where an unmet "this workspace has not
been built" belongs, so the message names `nros build` instead of surfacing four
frames deep in `cargo metadata` (the shape issue 0463 fixed for `nros sync`).

Whichever is chosen, the two leaf shapes should stop coexisting, and a gate
should say so — `every_standalone_rust_leaf_is_its_own_workspace_root` currently
neither requires nor forbids the table for a member, which is why `mixed` and
`launch` could diverge silently.

## Reproduce

```
rm -f examples/workspaces/launch/Cargo.toml    # already absent on a fresh clone
just format
```
