# Phase 315 — `system.toml` drives Rust selection too

**Status (2026-07-28): Draft.** Finishes what phase-314 started: the RMW, the
ROS edition and the capability list are declared once, in `system.toml`, for
**every** language. Closes phase-314's last open acceptance item and retires the
hand-written Rust selection it was working around.

## The asymmetry

The declaration is already universal and language-neutral:

```toml
# <bringup>/system.toml — the same file for C, C++ and Rust
[system]
name        = "demo"
rmw         = "zenoh"
ros_edition = "jazzy"
features    = ["safety", "param_services"]
```

What is not universal is **who consumes it**.

**C / C++** — derives everything. The user writes no selection at all:

```cmake
find_package(nano_ros REQUIRED)
nano_ros_add_executable(native_entry LAUNCH ...)
```

**Rust** — restates all of it by hand:

```toml
nros = { path = "…", default-features = false, features = [
    "alloc", "rmw-cffi",   # transport tier
    "ros-humble",          # edition — can contradict system.toml
    "param-services",      # capability — restates features = [...]
] }
nros-board-native = { path = "…", features = ["rmw-zenoh"] }   # RMW
```

Four restatements of three declared axes, and the copies can disagree:

| axis | on mismatch |
| --- | --- |
| edition | **silent wire mismatch** — codegen bakes jazzy hashes, the runtime speaks humble |
| capability | build error since phase-314; silent before it |
| RMW | usually a link error, sometimes the wrong backend |
| transport tier | build error |

There is also an invisible rule: a Rust NODE package must **not** name the
edition (cargo features are additive and the editions are
`compile_error!`-exclusive), while an ENTRY must. Two manifests that look alike
follow opposite rules, and nothing states it.

## Why Rust has no consumer today

Cargo resolves features from **manifests**, before any of our code runs. A
proc-macro can observe a mismatch — phase-314 made `nros::main!` assert on one —
but it cannot repair it, and `.cargo/config.toml` cannot express features.

So the derivation has to live in something generated that cargo reads as a
manifest. **That mechanism already exists on the C++ side**:
`nros_synth_runtime_umbrella` writes a generated crate (`nros_ws_runtime`) whose
`Cargo.toml` carries `nros-cpp = { features = [...] }`, which is exactly why a
C++ consumer names no features.

## Two shapes, two correct answers

The target UX is not "everything derives from `system.toml`" — it is "the user
states each choice once, in the place that owns it".

**Workspace** — a `system.toml` exists, so it owns the selection:

| | user writes |
| --- | --- |
| node pkg (C, C++, Rust) | nothing |
| C / C++ entry | `find_package(nano_ros)` + `nano_ros_add_executable(...)` |
| Rust entry *(this phase)* | a dep on the generated facade; no features |

**Standalone** — no `system.toml` exists, so the build command owns it, in each
toolchain's native selector:

```bash
cmake -S . -B build -DNANO_ROS_RMW=xrce -DNANO_ROS_ROS_EDITION=jazzy   # C/C++
cargo build --features rmw-xrce,ros-jazzy                              # Rust
```

Node packages are already correct in every language and both shapes. The gaps
are exactly two: the Rust workspace ENTRY (W1/W2) and the Rust standalone
edition default (W3).

## Design: a generated facade, never a user-manifest edit

`nros sync` generates a small facade crate per entry, carrying every selection
derived from `system.toml`. The entry depends on it; cargo's feature unification
propagates to `nros` and to the board crate.

```
<ws>/build/nros-sync/facade/<entry>/Cargo.toml     ← GENERATED
    nros              = { path = …, features = ["alloc","rmw-cffi","ros-jazzy","param-services"] }
    nros-board-native = { path = …, features = ["rmw-zenoh"] }
```

```toml
# the user's entry Cargo.toml — the target UX
[package.metadata.nros.entry]
deploy = "native"

[dependencies]
nros_selection = { path = "../../build/nros-sync/facade/native_entry" }
nros           = { path = "…", default-features = false }
nros-board-native = { path = "…" }
```

**`nros sync` never edits a user-authored `Cargo.toml`.** Generated selection
lives in generated code — the same boundary the C++ path already respects. The
one line the user writes is the dependency on the facade, and they write it once
when the package is created (the scaffold emits it).

## Waves

### W1 — the facade generator

`nros sync` emits the facade crate from the bringup's `system.toml`, using the
existing `capability_resolver` registry for `declared → nros_feature` and the
existing `resolve_rmw` SSoT for the backend. Mirrors
`nros_write_runtime_umbrella_crate`, which already does this for C++.

**Done when:** a workspace entry builds with the edition, capabilities and RMW
coming only from `system.toml`.

### W2 — migrate the workspace examples

Convert `examples/workspaces/*` Rust entries: drop the hand-written features,
add the facade dep. The node packages need no change — phase-314 already removed
their `ros-*`.

**Done when:** no workspace entry names an edition, a capability or an RMW
feature.

### W3 — standalone Rust: make the edition REPLACEABLE, not additive

Standalone examples were first drafted here as "decide whether to give them a
facade too". That framing was wrong, and checking what the other languages
actually do is what showed it.

**A standalone package has no `system.toml`, so selection is a build-time
input — and both toolchains already express that idiomatically:**

```bash
# C++ standalone: nothing in the file; the selector is -D
cmake -S . -B build -DNANO_ROS_RMW=xrce -DNANO_ROS_ROS_EDITION=jazzy

# Rust standalone: the selector is --features
cargo build --no-default-features --features rmw-xrce,ros-jazzy
```

That is the same UX in each toolchain's native selector, not a deviation to
tolerate. Standalone packages therefore get **no facade** — there is nothing to
derive from, and the copy-out-and-build property is what these examples exist to
demonstrate.

**But one thing IS a defect, and it is the same class phase-314 fixed
elsewhere.** The standalone Rust manifests carry an ADDITIVE edition default:

```toml
default = ["rmw-zenoh", "ros-humble"]
```

Cargo features are additive and `ros-{humble,iron,jazzy}` are
`compile_error!`-exclusive, so `--features ros-jazzy` yields BOTH and fails to
compile. The user has to know to pass `--no-default-features` and then re-name
every other default they still wanted. `-DNANO_ROS_ROS_EDITION=jazzy` simply
replaces the default; the Rust side has no equivalent.

So the C++ and Rust standalone selectors are NOT equivalent today: one is
replace-by-default, the other is add-and-conflict.

Fix: the edition must not be in `default`. Options, to be chosen with a
measurement rather than by taste:

1. **Drop `ros-*` from `default` and let no edition mean the default edition.**
   Zero editions is already legal (only >1 trips the `compile_error!`), which is
   what made phase-314 W3 safe for node packages. `--features ros-jazzy` then
   works with no `--no-default-features` dance. Needs confirmation that a
   zero-edition build really behaves as humble rather than as something
   undefined.
2. **Keep a default but make selection replace it** — not expressible in cargo
   without a `--no-default-features` step, so this is really "document the
   dance".

(1) is preferred and is the same move phase-314 made for node packages; the only
open question is what a zero-edition build resolves to, which W3 must verify
before applying.

Scope: 6 standalone manifests carry `default = [... ros-* ...]`; 95 name an
edition somewhere. Only the `default` list is in scope here — an explicit
`ros-humble = ["nros/ros-humble", …]` feature DEFINITION is the selector itself
and stays.

**Done when:** `cargo build --features ros-jazzy` works in a standalone example
with no `--no-default-features`, and W5's gate rejects an edition inside a
`default` list.

### W4 — retire the old paths

Once nothing consumes them:

- the hand-written `features = [...]` on `nros` / board deps in migrated
  entries;
- phase-314's `PARAM_SERVICES_ENABLED` assert — it exists to catch a
  hand-sync mistake that becomes impossible;
- the `posix` always-on special case in `nros_feature_set` (phase-314's recorded
  deviation), once every hosted example derives its capabilities;
- `NANO_ROS_FEATURES` as a user-facing cmake variable, if the facade makes it
  purely internal.

Each is only retired when the gate proves nothing depends on it. Retiring on
inspection is how the `_nros_runtime_platform_features` duplication survived —
it looked dead and was not.

**Done when:** each item above is either removed or has a recorded reason to
stay.

### W5 — gate the target UX

Extend `scripts/check-feature-set-ssot.sh` with two rules:

* no migrated WORKSPACE entry names an edition, capability or RMW cargo feature
  (the existing node-package check covers the other half);
* no manifest anywhere puts a `ros-*` feature in its `default` list — that is
  the additive-default trap from W3, and it is invisible until someone tries a
  second edition.

**Done when:** the gate fails when an entry restates a declared axis.

## Non-goals

- **Changing what any feature means.** This moves where selection is written.
- **Editing user manifests.** Explicitly rejected: generated selection belongs
  in generated code.
- **A new declaration format.** `system.toml` already carries all three axes;
  nothing new is invented.

## Acceptance

- [ ] A Rust entry names no edition, capability or RMW feature.
- [ ] `nros sync` writes no user-authored file.
- [ ] Changing `ros_edition` in `system.toml` changes what the Rust entry
      compiles against, with no manifest edit.
- [ ] `cargo build --features ros-<edition>` selects an edition in a standalone
      example without `--no-default-features`.
- [ ] Retired paths are removed, or their reason to stay is written down.
- [ ] The gate rejects an entry that restates a declared axis.
