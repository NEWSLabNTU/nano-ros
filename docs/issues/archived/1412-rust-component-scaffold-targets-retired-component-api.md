---
id: 1412
title: "`nros new --component --lang rust` emitted a package built on the `Component*`
  trait family retired in 212.N.12 — it compiled by neither documented route for
  three and a half months, and a rename alone would not have fixed it"
status: resolved
type: bug
area: [cli, api]
found: 2026-09-21
related: [1058, phase-452, phase-417, phase-307]
---

# The Rust half of issue 1058, in the language that issue never examined

[Issue 1058](../1058-scaffold-output-is-grepped-never-built.md) says the scaffold
tests verify every variant by substring match and never compile the result, and
proves it with the C++ template: phase-417 W-B3 renamed four type names to
`::rclcpp::` and three of the four did not exist. That half was fixed.

The Rust component template had the same disease and nobody looked. Found by
`scripts/check-scaffold-builds.sh` (phase-452 W2) on its first real run.

## What happens

```
$ nros new rc --component --lang rust
$ cd rc && NROS_REPO_DIR=<checkout> nros sync
sync: wrote [patch.crates-io] → .../rc/.cargo/config.toml
Error: refresh source metadata for `rc`
  metadata-mode harness failed (exit 101) for component 'rc::talker':
  error[E0432]: unresolved imports `nros::ComponentContext`, `nros::ComponentResult`
  error[E0405]: cannot find trait `Component` in crate `nros`
```

`nros sync` writes the `[patch.crates-io]` block correctly — the `version = "*"`
resolution was never the problem. It then fails because its metadata harness
compiles the component, and the component does not compile.

The package's two documented routes both fail. Its printed next-steps say to
join a workspace and run `nros metadata --build`; its emitted `Cargo.toml` says
the opposite — *"Standalone-buildable"*, via `nros sync` then `cargo build`.
Neither reaches a build.

## Why it was invisible

```
510992776  2026-06-03  refactor(212.N.12): retire Component* trait family + nros::component! macro
```

`Component` → `Node`, `ComponentContext` → `NodeContext`, `ComponentResult` →
`NodeResult`, and the `nros::component!` proc-macro forwarder DELETED. Every
callsite in the tree moved. The scaffold template did not, and `scaffold.rs`
has been edited since — most recently 2026-09-18 — so this is not an untouched
file. Nothing compiled what it emits.

Measured: `ComponentContext` appears only in `scaffold.rs` itself and one doc
comment in `metadata_build.rs`; `ComponentResult` only in `scaffold.rs`;
`pub trait Component` exists nowhere in the tree.

## A rename alone would NOT have fixed it — the phase-417 trap, repeated

Two of the five errors survive a pure rename, because 212.N.12 was a *name*
change for the trait family and an *API* change elsewhere:

| emitted | reality |
| --- | --- |
| `nros::Component` | retired → `nros::Node` |
| `ComponentContext` / `ComponentResult` | retired → `NodeContext` / `NodeResult` |
| `ctx.create_node(NodeId::new(…), NodeOptions::new(…))` | **arity wrong** — the two-arg form is `create_node_with_id`; `create_node` takes options only |
| *(no `ExecutableNode` impl)* | **required** — it declares a timer callback with no body, and a node instantiated into a generated binary must impl `ExecutableNode` |
| `nros::component!(…)` (trailing comment) | macro deleted → `nros::node!(…)` |

And two calls that look stale are fine: `create_publisher(EntityId, topic)` and
`create_timer(EntityId, CallbackId, period)` both still exist with exactly those
shapes, and `NodeId`/`EntityId`/`CallbackId` are still exported. So "rename the
Component names" produces something that still does not compile, and "rewrite
every call" would have changed two that were correct.

## Three further divergences the renewal also closes

1. **No `ExecutableNode` impl.** The template declared a timer callback
   (`cb_timer`) and emitted no body for it.
2. **A hand-rolled message.** It defined a `StringMsg` stand-in implementing
   `Serialize`/`Deserialize`/`RosMessage` by hand — against CLAUDE.md's
   "messages are generated … never hand-write" — while the C++ sibling uses the
   real `std_msgs::msg::Int32` and declares `<depend>std_msgs</depend>`. The
   Rust `package.xml` declared no dependencies at all.
3. **The type was named `Component`**, inside a `pub mod <use_case>`. The
   shipping shape is `impl Node for Class` at the CRATE ROOT with no module
   segment — which `nros::node!(Class)` and the manifest's `class` key both
   assume. Without a declared `class` the metadata harness falls back to
   guessing `<crate>::<module>::Component` and fails to compile; that fallback
   is documented in `metadata_build.rs` as surviving only for legacy manifests.

## Resolution

The template is renewed against the live API and matched to what the C++
component template demonstrates — same `std_msgs/Int32` on `/chatter`, same
1 Hz period, same counter state, a publish-and-report body, and
`<depend>std_msgs</depend>`. The node type sits at the crate root, `nros.toml`
declares its `class`, and `Cargo.toml` carries the `alloc` / `rmw-cffi` /
`macros` features plus the `std_msgs` and `log` deps the body needs.

The substring tests in `integration_tests.rs` were updated, NOT expanded: they
are the proxy issue 1058 is about, and the coverage now comes from
`scripts/check-scaffold-builds.sh`, which compiles every variant from a
scaffold made outside the checkout.

While updating them, one assertion failed on a *comment* in the emitted
`Cargo.toml` that contained the literal `[[bin]]` — a test meaning to check
structure, tripped by prose. A small live demonstration of the same point.

## Not fixed here

`nros.toml`'s `[component]` table is read by five CLI call sites but **no
shipped package uses it**: the four in-tree `nros.toml` files are CLI test
fixtures, while 31 example packages declare `[package.metadata.nros.node]` in
`Cargo.toml` instead. The renewed scaffold emits both. Which one a component is
supposed to carry is a separate question from this compile failure.
