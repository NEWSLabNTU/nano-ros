---
id: 311
title: "No SSoT for the cargo feature list: three cmake assemblies + every Rust leaf, two hardcoding `ros-humble` — blocks multi-edition and selectable capabilities"
status: resolved
type: bug
area: build
related: [0304, phase-308, phase-313]
fix_planned_in: phase-314
---

## Finding (phase-308 W1, 2026-07-28)

The cargo feature list for `nros-cpp` is built independently in two files:

| File | Path it serves |
| --- | --- |
| `cmake/NanoRosRuntimeCrate.cmake` (`set(_cpp_features …)`) | the synthesized `nros_ws_runtime` umbrella |
| `packages/core/nros-cpp/CMakeLists.txt` (`set(_cpp_features …)`) | direct `add_subdirectory` / import |

Same variable name, same crate, no shared source.

## How it failed

The phase-308 metadata probe needs `nros-cpp`'s `metadata-mode` feature. A
`NROS_EXTRA_CPP_FEATURES` hook was added to the umbrella path only. The probe
uses the direct path, so:

* the feature never reached any cargo invocation;
* `nros_cpp_metadata_dump` — `#[cfg(feature = "metadata-mode")]` — was absent
  from `libnros_cpp.a`;
* the probe failed at LINK with `undefined reference`, ~40 min into a build.

Nothing reported "that feature went nowhere". The hook looked correct in
isolation and was verified by a unit test that asserted the generated CMake
text — which contained the `set()` faithfully. Only `nm` on the built archive
showed the truth.

Fixed by adding the hook to both paths (`0304`). That leaves the duplication.

## Why it should be collapsed

Two independent assemblies of one crate's feature set means:

* every future consumer hook must be added twice, and the failure mode for
  forgetting is silent — a feature that simply does not apply;
* the two can drift on the *base* features too, which is worse than a missing
  hook: `nros-c` and `nros-cpp` resolving different `nros` features in one build
  is exactly the layout divergence the `defines_of` guard in
  `nros-build-helpers` now catches after the fact.

This is the same shape as two other defects found the same day — two writers of
`nros_config_generated.h`, and a recording backend that existed in three places
without being reachable from any of them. One logical thing, more than one
source, nothing checking they agree.

## Wider than two paths, and wider than C++

Measured 2026-07-28. The feature set is assembled independently in **three**
cmake sites, and the ROS edition is hardcoded in two of them:

| site | edition | rmw | platform | capabilities |
| --- | --- | --- | --- | --- |
| `nros-cpp/CMakeLists.txt` | **hardcoded `ros-humble`** | inline copy | inline chain (+ `NANO_ROS_BOARD` threadx split) | param/lifecycle on posix, safety-e2e |
| `nros-c/CMakeLists.txt` | **hardcoded `ros-humble`** | inline copy | inline chain | — |
| `NanoRosRuntimeCrate.cmake` | `ros-${_NRR_EDITION}` ✔ | `nros_rmw_dispatch()` SSoT ✔ | `_nros_runtime_platform_features()` | none |

Plus five `cmake/platform/*.cmake` files carrying their own `platform-*`
knowledge, and every Rust leaf `Cargo.toml` naming `"ros-humble"` by hand.

Only the umbrella honours the configured edition (phase-304 W2b, RFC-0056); the
other two were never updated. RFC-0056 makes the edition drive the runtime
keyexpr format so it matches the codegen-baked `type_hash`, so a non-humble
build through either direct path compiles the runtime as humble while codegen
bakes iron/jazzy hashes — a WIRE MISMATCH, not a build error.

## The Rust side blocks multi-edition outright

`packages/core/nros/src/lib.rs:110`:

```rust
compile_error!("`ros-{humble,iron,jazzy}` are mutually exclusive — select one ROS edition.");
```

Cargo features are additive and unify across a build. A leaf naming
`nros = { features = ["ros-humble"] }` does not express a default that an entry
can override — it *adds* `ros-humble` to the unified set. So an entry selecting
`ros-jazzy` in a workspace whose leaves say `ros-humble` gets both, and the
build fails the `compile_error!`.

**Every Rust node package in the tree names its edition today.** Multi-edition
support therefore cannot work by "setting the edition somewhere"; the leaves
have to stop naming it at all.

That inverts the usual instinct: the edition is not a per-package choice, it is
a per-IMAGE one. Feature unification already makes it so — the entry (or
umbrella) is the only place that can hold it consistently.

The same argument applies to selectable capabilities (`param-services`,
`lifecycle-services`, `safety-e2e`): they are image-level, currently expressed
as platform-conditional side rules on ONE of the three cmake paths.

## Requirement: one feature SSoT, all languages

The list must be derived once from `(edition, rmw, platform, board,
capabilities)` and consumed by every front-end — the C path, the C++ path, the
umbrella, the Zephyr lane, and the Rust bake. Same property phase-308 already
established for the metadata recorder and the slot accounting: one mechanism,
several front-ends.

## Reconciliation path

**Planned as [phase-314](../roadmap/phase-314-feature-set-ssot.md).** The wave
breakdown there follows the order below.

Order matters — collapsing first would change behaviour silently, since the
three sites do not agree today.

1. **Decide each divergence deliberately**, not by picking a winner:
   - *edition* — direct paths must honour `NANO_ROS_ROS_EDITION` (it already
     exists and drives codegen). This is a defect fix, not a preference.
   - *rmw* — the dispatch SSoT (`nros_rmw_dispatch`) wins over the inline
     copies; it is already the resolve_rmw single source.
   - *platform* — the umbrella's helper is WEAKER: it lacks the
     `NANO_ROS_BOARD` disambiguation for threadx (`threadx-linux` = std,
     `riscv64-qemu` = no_std). Unifying naively regresses it. The direct
     chain's logic is the one to keep.
   - *capabilities* — decide whether the umbrella's omission of
     `param-services` / `safety-e2e` is intentional or an oversight. This is
     the one question that needs someone who knows the intent.
2. **Extract the agreed computation** into one cmake function taking
   `(edition, rmw, platform, board, capabilities)` and returning the list. The
   three sites become callers; `NROS_EXTRA_CPP_FEATURES` applies once, by
   construction, and the "add the hook twice" trap disappears.
3. **Drop the edition from Rust leaves.** They stop naming `ros-*`; the entry
   or umbrella supplies it. Until this lands, multi-edition is impossible in a
   workspace regardless of what cmake does.
4. **Gate it.** Assert the C, C++ and umbrella paths produce the same list for
   the same inputs. Without that, they drift again — the failure this issue was
   filed for was silent, and so is drift.

Steps 1 and 3 are the substance; step 2 is mechanical once they are settled.

## Options considered for step 2

1. **One cmake function, several callers.** Recommended. Smallest change that
   removes the trap; keeps every entry point working.
2. **One path** — force direct consumers through the umbrella. Cleaner, but the
   umbrella exists for workspace builds and imposing it on standalone consumers
   is not free.
3. **Assert agreement only.** Cheapest, keeps the duplication, catches drift
   but not a missing hook — which is what actually bit.

## Resolved (2026-07-28) — phase-314

`cmake/NanoRosFeatureSet.cmake` is the single computation; `nros-c`, `nros-cpp`
and the workspace umbrella call it. ~180 lines of duplicated mapping removed.

What changed behaviourally, all of it deliberate:

* the direct paths honour `NANO_ROS_ROS_EDITION` instead of hardcoding
  `ros-humble`, so a non-humble build no longer compiles the runtime as humble
  while codegen bakes other type_hashes;
* the umbrella gained the `NANO_ROS_BOARD` threadx split it never had — the
  direct chain's logic won, because unifying onto the umbrella's helper would
  have REGRESSED it;
* the umbrella gained capabilities, closing the gap where a mixed workspace
  lost `param-services` that a pure C/C++ workspace kept;
* `NROS_EXTRA_CPP_FEATURES` applies once, so the add-it-twice trap behind issue
  0304 cannot recur;
* 50 Rust node packages stopped naming a ROS edition — cargo features are
  additive and the editions are `compile_error!`-exclusive, so a leaf naming one
  made every other edition unbuildable in that workspace.

Two things the analysis got wrong and the implementation corrected:

* **nros-c and nros-cpp spell the RMW selection differently**
  (`cffi-zenoh-cffi` / `cffi-xrce-c` vs `rmw-zenoh-cffi` / `rmw-xrce-cffi`). The
  shared function takes a `CRATE` argument rather than callers post-processing.
  Renaming the features to match is a separate change.
* **`cmake/board/*` and `cmake/platform/*` were NOT a fourth duplication.** The
  matches were directory names and `NROS_PLATFORM_LINK_FEATURES` (a transport
  axis). Grepping a feature name instead of an assignment produced the false
  claim.

Gated by `scripts/check-feature-set-ssot.sh` in `just check`, which found 25
packages the manual sweep had missed (inline feature arrays the edit's regex did
not match).
