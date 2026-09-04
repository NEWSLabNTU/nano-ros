# Phase 315 — `system.toml` drives Rust selection too

**Status (2026-09-04): COMPLETE.** W1, W2, W3, W5 landed and verified
2026-07-30; W4's one open decision — the `posix` always-on — was taken by
**phase-323 W2** on 2026-07-31, whose workstream is titled "delete the `posix`
always-on". This doc never recorded it. Verified against `main` 2026-09-04, see
the acceptance item for the evidence. Finishes what phase-314 started: the RMW, the
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
nros-board-linux = { path = "…", features = ["rmw-zenoh"] }   # RMW
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
    nros-board-linux = { path = …, features = ["rmw-zenoh"] }
```

```toml
# the user's entry Cargo.toml — the target UX
[package.metadata.nros.entry]
deploy = "native"

[dependencies]
nros_selection = { path = "../../build/nros-sync/facade/native_entry" }
nros           = { path = "…", default-features = false }
nros-board-linux = { path = "…" }
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
coming only from `system.toml`. — **DONE.** Setting `ros_edition = "jazzy"` and
re-running sync changes what the entry compiles against with no manifest edit;
`cargo tree` confirms `FEATS=alloc,rmw-cffi,ros-jazzy,std` reaches `nros`.

Two things implementation found that the design had assumed away:

**Entries come from cargo's `members`, not from the ament scan.** Nine entries
have no `package.xml`, so `nros sync`'s scan never saw them — they are cargo
workspace members and nothing else, which is legal (the workspace root is their
patch authority). Keying facade generation off the scan skipped exactly those
nine, and the skip was invisible: sync succeeded and they kept their
hand-written features, which is the state that looks correct. Cargo's member
list is the right truth here, since the mechanism IS cargo feature unification.

**A direct `nros-rmw-*` dep needs the edition too.** Some entries depend on the
backend crate directly rather than only through the board, and the backend's
own `keyexpr` module cfg-gates the RIHS01 type-hash tail on `ros-iron` /
`ros-jazzy`. Miss that forward and a jazzy build keeps the humble
`TypeHashNotSupported` placeholder on the wire while compiling clean — this
phase's exact failure mode, reintroduced one dependency to the left. The
facade emits `nros-rmw-*` alongside `nros` and the board crate.

### W2 — migrate the workspace examples

Convert `examples/workspaces/*` Rust entries: drop the hand-written features,
add the facade dep. The node packages need no change — phase-314 already removed
their `ros-*`.

**Done when:** no workspace entry names an edition, a capability or an RMW
feature on a dep the facade owns. — **DONE**, 35 manifests.

Two restatements survive migration and are W4's, not W2's:

* the zephyr entries keep a local `[features] default = ["rmw-zenoh"]` — the
  entry's own feature namespace, but still a second RMW selection;
* node packages forward capabilities (`safe_listener_pkg/safety-e2e`), which
  duplicates `system.toml`'s `features`. Node-level capability derivation is
  its own design question — a node package is linked into someone else's image
  and cannot see the bringup.

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
`default` list. — **DONE.** Option (1) taken; the open question resolved in its
favour. Zero editions is not undefined: every consumer gates on
`cfg(not(any(ros-iron, ros-jazzy)))` — `keyexpr.rs` does so in five places — so
no edition IS humble by construction. Verified in both directions: restoring the
old default reproduces the `compile_error!`, dropping it makes
`--features ros-jazzy` build.

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

### W4 scoping (2026-07-28, read-only — no code changed yet)

Two of the four candidates are **not** dead, and one of them the wave had
mis-classified:

**`PARAM_SERVICES_ENABLED` stays.** The reasoning for retiring it was "the
facade makes the hand-sync mistake impossible". True for a WORKSPACE entry, and
only there. A standalone entry has no `system.toml` and therefore no facade, so
the assert is still the only thing standing between a declared
`[param_services]` and an `nros` built without the feature. Only two references
exist (the const and the assert), which is what made it look dead.

**`NANO_ROS_FEATURES` stays, and is not "purely internal".** It is set by
GENERATED code for workspaces (`codegen_system.rs` bakes it into
`system_config.cmake`), which is what suggested it was internal — but it is also
set by hand in standalone examples:

```cmake
# examples/native/{c,cpp}/safety-listener/CMakeLists.txt
set(NANO_ROS_FEATURES "safety")
```

That is the standalone capability selector: the C/C++ twin of what W3
established for Rust editions, and the same shape as `-DNANO_ROS_RMW`. A
standalone package has no declaration to derive from, so the build files ARE the
declaration. Retiring it would remove the only way a standalone C/C++ example
can ask for a capability.

**The `posix` always-on special case is a real candidate, but not on
inspection.** `NanoRosRuntimeCrate.cmake:230` appends `param_services lifecycle`
for every posix build. Exactly 9 workspaces declare a capability (params /
lifecycle / safety × c / cpp / rust); the other ~26 get these two features
without asking. Since the 9 that need them already declare them, the line looks
removable — but "looks removable" is precisely how the
`_nros_runtime_platform_features` duplication survived. Removing it must be
proven by a fixture sweep, not by this reasoning, because the failure mode is a
capability quietly missing from an image rather than a build error.

Note the near-miss: W2's migration strips `param-services` from entry manifests,
and `ws-params-rust`'s `system.toml` has no `features =` line — which looks like
a dropped capability until you notice `capability_enabled()` also honours the
deprecated typed `[param_services]` block, which is the form these 9 use. The
facade does carry it. A `grep '^features'` audit of these files reports the
wrong answer.

### W5 — gate the target UX

Extend `scripts/check-feature-set-ssot.sh` with two rules:

* no migrated WORKSPACE entry names an edition, capability or RMW cargo feature
  (the existing node-package check covers the other half);
* no manifest anywhere puts a `ros-*` feature in its `default` list — that is
  the additive-default trap from W3, and it is invisible until someone tries a
  second edition.

**Done when:** the gate fails when an entry restates a declared axis. — **DONE.**

Rule 5 needed a parser rather than a grep. A line-wise match cannot tell "the
entry sets `nros/ros-humble`" from "a node package forwards its own
`safety-e2e`", and only the first is this phase's to fix; it also flagged every
entry for its own PROSE, since these manifests explain the axes at length right
where they used to set them. The rule now walks brace-balanced dep specs,
scoped to the deps the facade owns, with comments stripped.

## Non-goals

- **Changing what any feature means.** This moves where selection is written.
- **Editing user manifests.** Explicitly rejected: generated selection belongs
  in generated code.
- **A new declaration format.** `system.toml` already carries all three axes;
  nothing new is invented.

## Acceptance

- [x] **A Rust entry names no edition, capability or RMW feature** — on the deps
      the facade owns (`nros`, `nros-board-*`, `nros-rmw-*`). 35 manifests
      migrated; the W5 gate reports 0 violations.

      Recorded exception: the **6 zephyr entries keep a local
      `[features] default = ["rmw-zenoh"]`**. That is not a restatement — it is
      the ONLY RMW selector for a board crate that has no `rmw-*` feature at all
      (`nros-board-zephyr` declares `tiers` / `zephyr-edf` and nothing else), and
      the west build passes `--no-default-features --features rmw-zenoh` on the
      cargo line. W4 originally listed it for removal; doing so would have broken
      every zephyr entry.
- [x] **`nros sync` writes no user-authored file** — the facade is generated into
      `<ws>/generated/nros-selection/<entry>/`, which is gitignored. The one line
      the user writes is the dep on it.
- [x] **Changing `ros_edition` changes what the Rust entry compiles against, with
      no manifest edit** — set `ros_edition = "jazzy"`, re-ran sync, `cargo tree`
      reports `FEATS=alloc,rmw-cffi,ros-jazzy,std` on `nros`.
- [x] **`cargo build --features ros-<edition>` works without
      `--no-default-features`** — verified on `examples/native/rust/talker`, and
      verified in the negative: restoring the old default reproduces
      `error: ros-{humble,iron,jazzy} are mutually exclusive`.
- [x] **Retired paths removed, or their reason to stay written down** — all
      four resolved, all by evidence rather than inspection:
      - `PARAM_SERVICES_ENABLED` **stays** — the facade only covers workspace
        entries; a standalone entry has no `system.toml`, so the assert remains
        the only guard there.
      - `NANO_ROS_FEATURES` **stays** — generated for workspaces, but set BY HAND
        in `examples/native/{c,cpp}/safety-listener`. It is the standalone
        capability selector, the C/C++ twin of W3's Rust edition selector.
      - the zephyr local `default` **stays**, per the exception above.
      - the `posix` always-on `param_services lifecycle` in
        `NanoRosRuntimeCrate.cmake` is **REMOVED** — by phase-323 W2
        (2026-07-31, "delete the `posix` always-on"), not by this phase.
        Confirmed on `main` 2026-09-04: no unconditional capability append
        survives, and `cmake/NanoRosRuntimeCrate.cmake:226-234` carries the
        removal's reasoning where the code used to be — *"posix used to get
        `param_services` + `lifecycle` unconditionally … W1 made
        `nano_ros_workspace()` resolve the axes from `SYSTEM` before the import,
        so the declaration now arrives on its own and this can go."*

        The blocker named here — "the failure mode is a capability silently
        missing from an image, not a build error, so it needs a fixture sweep to
        decide" — was dissolved rather than paid: phase-323 W4 gated it, and the
        failure is no longer silent in either direction. A system that declares
        `[param_services]` against an entry that lacks the feature is a
        const-eval panic from `nros::main!`
        (`nros-macros/src/main_macro.rs:1228`, phase-314's guard), and a missing
        facade is now fatal at `nros build` rather than a warning
        (`nros-cli-core/src/cmd/build.rs`, phase-413 W2). phase-323 did run the
        measurement round this item asked for — "All measured on real
        workspaces, with the posix always-on removed".

        *Original wording, kept as the record of what was true on 2026-07-30:*
        OPEN. 9 workspaces declare a capability; ~26 received these two without
        asking. The 9 that need them already declare them, so it looked
        removable — but the failure mode was a capability silently missing from
        an image, not a build error, so it needed a fixture sweep to decide.
- [x] **The gate rejects an entry that restates a declared axis** — W5 rule 5,
      brace-balanced over facade-owned dep specs, comments stripped.

## What the fixture sweep found (2026-07-28/29)

The sweep was gated ahead of W4 deliberately, and it overturned two W4
assumptions that would otherwise have shipped as breakage (the zephyr `default`
and `NANO_ROS_FEATURES`, both above). It also surfaced three defects, two of them
introduced by this phase:

* **the facade emitted features the target crate does not declare** — fixed in
  W1; 18 of 23 board crates carry `rmw-*`, five do not;
* **`kind = "embedded"` deploy blocks were treated as placements** — fixed in the
  resolver. Wider than this phase: `ws-realtime-c` and `ws-realtime-cpp` could not
  be re-resolved at all, masked by pre-0.1.0 committed models (issue 0320);
* **stale build dirs**, diagnosed to CLAUDE.md's documented wipe rule rather than
  to a feature asymmetry — confirmed by wiping one workspace and watching the
  failure move to the next unwiped one.
