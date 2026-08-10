---
rfc: 0071
title: "RMW backend descriptor: a backend declares itself, core names none"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# RFC-0071 — RMW backend descriptor: a backend declares itself, core names none

## Summary

A backend package ships an `nros-rmw.toml` descriptor declaring its build facts
and its capabilities. The toolchain lowers that descriptor the way it already
lowers a board's `nros-board.toml` (RFC-0042 D2). Core packages stop naming
backends: the cfgs they already gate on are flipped by **declared capabilities**
instead of by **detected backend features**, and the closed `resolve_rmw` match
plus its generated CMake `if/elseif` chain are replaced by a read of the
descriptor.

The runtime seam does not change. `nros_rmw_vtable_t` (RFC-0035) already carries
C, C++, Rust and mixed backends and is not the gap.

## Motivation

### "CycloneDDS is the exception" is wrong twice

The exception is real but it is neither singular nor for the reason usually
given. RFC-0031 says cyclonedds "is not pure-cargo linkable" — the *opposite* of
being cargo-linkable — and the tree shows it is not alone:

| backend | implementation | Cargo.toml | CMakeLists.txt |
| --- | --- | --- | --- |
| zenoh | Rust (+ zenoh-pico C via `build.rs`) | yes | no |
| **xrce** | **C** (`publisher.c`, `service.c`) | no | yes |
| **uorb** | **C++** (`publisher.cpp`) | no | yes |
| cyclonedds | mixed (`descriptors.cpp`, `bridge.rs`) | yes | yes |

Three of four are already not pure-cargo. What makes cyclonedds *feel* singular
is not its language but that it is the only one demanding **per-message-type
work** — and that demand is what leaked into core.

### Core names backends today

Two sites, both in `packages/core/nros-node/build.rs`:

```rust
let has_rmw = env::var("CARGO_FEATURE_RMW_ZENOH").is_ok()
    || env::var("CARGO_FEATURE_RMW_XRCE").is_ok()
    || env::var("CARGO_FEATURE_RMW_CFFI").is_ok()
    || env::var("CARGO_FEATURE_RMW_UORB").is_ok();
…
if env::var("CARGO_FEATURE___CYCLONEDDS_LINK").is_ok() {
    println!("cargo:rustc-cfg=rmw_needs_type_descriptors");
}
```

**The capability is already generic; only the trigger is backend-named.**
`rmw_type_registry.rs` gates on `rmw_needs_type_descriptors` and exposes
`MessageForRmw` — a capability name and a generic seam. Core is not asking "is
this Cyclone?" in its logic; it is asking it only in its *detection*. Phase-248
C2 got the seam right and left the trigger behind.

Elsewhere the naming is load-bearing rather than vestigial:

* `packages/api/nros-c/Cargo.toml` declares `rmw-zenoh` / `rmw-xrce` /
  `rmw-cyclonedds`, with `dep:` on concrete backends — and the asymmetry IS the
  special case: `rmw-cyclonedds = ["rmw-cffi"]` carries no `dep:`, because
  Cyclone arrives through CMake. A user-facing library encodes one backend's
  build route in its feature table.
* `packages/cli/cargo-nano-ros/src/rmw_resolver.rs` holds a closed `KNOWN_RMW`
  list, an `UnknownRmw` error, and a `canonical_rmw` alias `match`.
* `cmake/NanoRosRmwDispatch.cmake` is generated from that match into an
  `if/elseif` chain ending in `FATAL_ERROR: unknown rmw`.
* `cmake/NanoRosGenerateInterfaces.cmake` mentions cyclonedds **27 times** —
  more than any other file — for the per-message IDL descriptor step.

### The closed list has already failed

`nros_rmw_dispatch` reports "known: zenoh xrce cyclonedds". **`uorb` is absent**,
though it is an in-tree backend. A closed enum has already stopped covering the
tree it governs, which is the strongest argument that the list should not be a
list.

## Design

### The precedent to copy: boards

Boards solved this problem already. `nros-board.toml` declares
`[board.capabilities]` and `capability_features`, and
`cmake/NanoRosCapabilities.cmake` lowers them into `NROS_PLATFORM_HAS_*`
defines, "so the cmake-driven fixture builds derive them from board.toml instead
of hand-setting them per overlay (the issue-0038 footgun)".

This RFC is that mechanism applied to the RMW axis. It invents nothing.

### D1 — `nros-rmw.toml`, shipped by the backend

```toml
[rmw]
names = ["cyclonedds", "rmw-cyclonedds", "rmw-cyclonedds-cffi"]  # alias table, was canonical_rmw()
cffi_feature = "rmw-cyclonedds-cffi"

[rmw.build]
driver          = "cmake"          # cargo | cmake | both
rlib_dep        = ""               # bundled in the umbrella, or absent
extra_link_libs = ["nros_rmw_cyclonedds", "ddsc", "stdc++"]
needs_cxx_linker = true

[rmw.capabilities]
type_descriptors = true            # -> cfg(rmw_needs_type_descriptors)

[rmw.codegen]
per_message = "nros_rmw_cyclonedds_generate_from_msg"   # see D4
```

`[rmw.build]` is exactly the four fields `nros_rmw_dispatch` already computes —
relocated from a central match to the package they describe. The zenoh
descriptor is the same file with `driver = "cargo"`, no extra libs, and no
capabilities.

### D2 — core receives capabilities, never backend names

`nros-node` gains two capability features, and `build.rs` stops enumerating
backends:

| today | after |
| --- | --- |
| `CARGO_FEATURE_RMW_{ZENOH,XRCE,CFFI,UORB}` → `has_rmw` | `rmw-present` → `has_rmw` |
| `CARGO_FEATURE___CYCLONEDDS_LINK` → `rmw_needs_type_descriptors` | `needs-type-descriptors` → `rmw_needs_type_descriptors` |

`__cyclonedds-link` is deleted. Nothing else in core moves, because the seam it
feeds (`MessageForRmw`, `register_type_descriptor`) is already generic.

### D3 — the selection facade carries the lowering

The translation point already exists and is already generated. `nros sync`
writes `<entry>_nros_selection` (RFC-0031's lowering made concrete), which today
carries the board's `rmw-<x>` feature. It gains the capability features read
from the descriptor:

```toml
[dependencies]
nros = { path = "…", default-features = false, features = ["ros-humble"] }
nros-node = { path = "…", features = ["rmw-present", "needs-type-descriptors"] }
nros-board-linux = { path = "…", features = ["rmw-cyclonedds"] }
```

No user manifest changes; cargo unifies the features onto the packages the entry
already depends on. A third-party backend needs no core edit — its descriptor
supplies the same rows.

### D4 — the hard half: per-message codegen

`[rmw.codegen].per_message` names a CMake function the backend's own
`CMakeLists.txt` defines. `nros_generate_interfaces()` replaces

```cmake
if(NANO_ROS_RMW STREQUAL "cyclonedds"
   AND COMMAND nros_rmw_cyclonedds_generate_from_msg)
```

with a call through the descriptor: if the active backend declares
`per_message`, invoke that command per message type; otherwise do nothing. The
`COMMAND` guard is retained — a backend that declares a hook it did not define
is a configure-time error naming the backend, not a silent skip.

This is the part that is a **hook, not a data field**, and it is the piece with
genuine design risk: it makes the codegen pipeline call backend-supplied code.
The mitigation is that the call site is one, the contract is one function
signature, and a backend that declares nothing pays nothing.

### D5 — the resolver reads descriptors

`resolve_rmw` keeps its shape and loses its `match`: it discovers descriptors
(in-tree backends, plus any declared by the workspace) and resolves the declared
name through their `names` tables. `UnknownRmw` then means "no descriptor claims
this name" and can list what was found, which is a better error than a frozen
constant. `NanoRosRmwDispatch.cmake` stops being generated code and becomes a
loop over descriptors.

## What this does NOT change

* **The vtable ABI.** RFC-0035 is not the gap. A backend still populates
  `nros_rmw_vtable_t` and self-registers through `RMW_INIT_ENTRIES`.
* **Requiring a Cargo manifest from every backend** — explicitly rejected.
  `nros-rmw-xrce` (C) and `nros-rmw-uorb` (C++) have no `Cargo.toml` and should
  not gain one; mandating it would force cargo-driven builds and destroy the
  language neutrality the C ABI buys. The descriptor is a plain declarative file,
  in the `nros-sdk-index.toml` / `nros-board.toml` idiom, not any language's
  build manifest.
* **Cyclone's CMake route.** It stays; it stops being *special*. A descriptor
  saying `driver = "cmake"`, `needs_cxx_linker = true` is a backend describing
  itself, not the toolchain knowing its name.

## Verification

The point of the RFC is falsifiable by grep, which is how it should be gated:

* `packages/core/**` and `packages/api/nros{,-c,-cpp}/**` contain no
  `zenoh|xrce|cyclonedds|uorb` outside prose/comments — a `check-rmw-agnostic`
  gate in the spirit of `check-ffi-struct-mirrors`.
* `nros_rmw_dispatch` has no backend name in it.
* Adding a backend touches no file outside its own package plus one descriptor
  discovery path. **The acceptance test is a fifth backend added out-of-tree
  with zero core edits** — until that is demonstrated the RFC is unproven,
  because every closed list in this area looked open until someone tried.

## Open questions

1. **Descriptor discovery.** In-tree backends are findable by convention; an
   out-of-tree one needs a declared path (workspace `system.toml`, or the SDK
   index). Unresolved.
2. **`has_rmw` semantics.** Today it is also true under `cfg(test)` for
   `MockSession`. A `rmw-present` feature must keep that working without a
   backend present.
3. **uorb's absence from the dispatch** — is it a gap to fix in passing, or does
   the PX4 path deliberately bypass `nros_rmw_dispatch`? Answer before
   generalising, because it decides whether the descriptor set must cover it.
4. **Interaction with issue 0493.** The provider/identity work is in flight and
   touches the same `nros-c`/`nros-cpp` feature tables. Sequence, do not
   parallelise.
