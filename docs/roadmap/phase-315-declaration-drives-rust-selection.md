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

### W3 — standalone examples: decide, then migrate

~50 standalone Rust examples are their own entry with no workspace, and today
build with a bare `cargo build`. A facade makes `nros sync` a prerequisite for
them, which is a real UX change — for workspaces it is already true (patch
tables, generated interface crates); for these it is new.

Two options, to be decided IN this wave rather than assumed:

1. **Facade for them too** — full consistency, at the cost of `cargo build`
   alone no longer working in an example directory.
2. **Exempt them** — a standalone example is a single image with no `system.toml`
   to derive from, so hand-written selection is arguably honest there. Keeps the
   copy-out-and-build property the examples exist to demonstrate.

Recommendation: (2). The declaration these examples would derive from does not
exist for them, and the phase's goal is "declare once", not "generate always".
But it means the retirement in W4 is scoped to workspaces.

**Done when:** the decision is recorded here with its reasoning, and the chosen
option is implemented.

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

Extend `scripts/check-feature-set-ssot.sh`: no migrated entry may name an
edition, capability or RMW cargo feature. The existing node-package check
already covers the other half.

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
- [ ] The standalone-example decision is recorded with reasoning.
- [ ] Retired paths are removed, or their reason to stay is written down.
- [ ] The gate rejects an entry that restates a declared axis.
