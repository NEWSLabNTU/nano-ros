# Phase 87: Rust-as-SSoT sizes for C/C++ opaque storage

**Goal**: Replace every hand-coded opaque-storage size in the workspace
with values sourced from Rust's `core::mem::size_of`. The `nros` umbrella
crate owns the size constants; `nros-c` / `nros-cpp` read them at build
time by probing `nros`'s compiled rlib via the `object` crate.

**Status**: Not Started
**Priority**: Medium — unblocks 3 NuttX rtos_e2e cases that currently
trip a compile-time assert on 32-bit targets, and removes an entire
class of drift bug across 4 build scripts.
**Depends on**: Phase 85.4 (rtos_e2e parametrised tests; currently the
loudest failure mode for the undercount bug).
**Supersedes**: Phase 85.9 (the "stopgap bump" path explicitly rejected
by the project owner: *"we should avoid manual size calculation. Instead,
the size should be generated in compile time"*).

## Overview

### Problem

Every opaque C/C++ wrapper in the project embeds a fixed-size byte array
sized by hand-math in build scripts:

```cpp
// nros-cpp/include/nros/publisher.hpp
alignas(8) uint8_t storage_[NROS_CPP_PUBLISHER_STORAGE_SIZE];
```

```c
// nros-c generated header / types.h
uint64_t _opaque[NROS_PUBLISHER_OPAQUE_U64S];
```

The backing constants come from four different places, none of which
agree or are authoritative:

1. **`nros-cpp/build.rs`** — pointer-width arithmetic: `4 * ptr_bytes +
   name_buf + ptr_bytes + 4 * ptr_bytes` per entity. Under-counts on
   32-bit ARM → trips `const _: () = assert!(size_of::<T>() <= STORAGE)`
   in `nros-cpp/src/lib.rs:350` → blocks NuttX C/C++ builds.
2. **`nros-c/build.rs`** — hand-picked literals: `session_upper = 512`,
   `entries_upper = max_cbs * 80`, `action_server_storage_bytes = 256`.
   Works only because the literals are deliberately generous.
3. **`nros-c/src/opaque_sizes.rs`** — the only place that uses real
   `u64s_for::<T>()`, but cbindgen drops it (issue #252) so the
   generated header shows `#define PUBLISHER_OPAQUE_U64S 1` — a
   placeholder that no C code should consume.
4. **`nros-c/include/nros/types.h`** — hand-maintained magic numbers
   (`#define NROS_PUBLISHER_OPAQUE_U64S 48`). What C examples actually
   link against. Drifts silently from (3).

Every one of these needs an update any time a backend's handle type
grows a field. The drift cost is real: the 32-bit ARM undercount has
been blocking tests for weeks.

### Design

Three principles, picked jointly with the project owner:

1. **Rust is the single source of truth for sizes.** Each relevant type
   has exactly one `pub const FOO_SIZE: usize = core::mem::size_of::<T>();`
   entry. That const is the contract; C/C++ values are derived from it.

2. **`nros` (the umbrella crate) hosts the exports.** Users and
   consumer build scripts look in one place — not per-backend crates
   that pollute the dep graph with size-only edges.

3. **Generation happens during normal `cargo build`.** No separate
   codegen tool, no `cargo nros-headers` subcommand, no external
   pipeline. `nros-c/build.rs` and `nros-cpp/build.rs` probe `nros`'s
   compiled rlib with the `object` crate, format the values into their
   respective headers.

Further, the consumer wrappers go on a diet:

4. **The C++ wrappers become thin.** Today `CppPublisher` is a Rust
   struct bundling `RmwPublisher + [u8; 256] name + usize len`. The
   metadata moves to the C++ class; the opaque storage shrinks to just
   `size_of::<RmwPublisher>()`. Both nros-c and nros-cpp end up with
   the *same* opaque size per Rust handle, sourced from the same const.

5. **C-internal FFI shims become `#[repr(C)]`.** Types like
   `ActionServerInternal` and `ServiceClientInternal` are shim structs
   of pointers and small integers — there's no reason they can't be
   `#[repr(C)]`. Once they are, cbindgen emits the C struct directly
   and the C compiler computes `sizeof()` natively. No probe needed for
   these.

### Why the `object` crate, not `llvm-nm`

Two options for reading symbol sizes out of the rlib:

- **`llvm-nm --print-size --defined-only`** — used by
  `zpico-platform-shim/build.rs` today for probing C struct sizes.
  Subprocess, string parsing, path-finding for rustc's bundled llvm-nm.
- **`object` crate (gimli-rs)** — pure Rust, typed API. Parses ar
  archives natively, dispatches per-member to ELF/Mach-O/COFF readers,
  exposes `ObjectSymbol::size()` directly. What rustc itself uses
  internally for object-file reading. No subprocess, no string parsing,
  no path-hunting.

We pick `object`. The probe is 30 lines, no external tool dependency,
typed error handling. `zpico-platform-shim`'s llvm-nm-based probe
stays for now; a follow-up can migrate it for consistency.

### Crate graph (after Phase 87)

```
 ┌──────────────────────────────────────────────┐
 │ nros-rmw-zenoh, nros-rmw-xrce, nros-node     │  unchanged — define
 │                                              │  the real Rust types
 └──────────────────────┬───────────────────────┘
                        │
                        ▼
 ┌──────────────────────────────────────────────┐
 │ nros (umbrella)                              │
 │   src/sizes.rs  ← SSoT                       │
 │     pub const PUBLISHER_SIZE = size_of::<T>();
 │     #[used] static __NROS_SIZE_PUBLISHER:    │
 │                   [u8; PUBLISHER_SIZE] = [0; _];
 │                                              │
 │   feature axes match today's — `rmw-zenoh`   │
 │   / `rmw-xrce` / `rmw-dds` flip in which     │
 │   concrete type T resolves to via the        │
 │   `RmwPublisher = <ConcreteSession as        │
 │    Session>::PublisherHandle` alias          │
 └──────────┬──────────────────────┬────────────┘
            │                      │
            ▼                      ▼
 ┌──────────────────────┐ ┌──────────────────────┐
 │ nros-c               │ │ nros-cpp             │
 │   build.rs reads     │ │   build.rs reads     │
 │   nros's rlib via    │ │   nros's rlib via    │
 │   nros-sizes-build   │ │   nros-sizes-build   │
 │   → writes           │ │   → writes           │
 │   nros_config_       │ │   nros_cpp_config_   │
 │   generated.h        │ │   generated.h        │
 │                      │ │   (thin-wrapper      │
 │   types.h drops      │ │    refactor removes  │
 │   hand-maintained    │ │    CppPublisher etc.)│
 │   size consts        │ │                      │
 └──────────────────────┘ └──────────────────────┘
            ▲                      ▲
            │                      │
            └──────────┬───────────┘
                       │
                       ▼
 ┌──────────────────────────────────────────────┐
 │ nros-sizes-build (NEW build-dep utility)     │
 │   fn find_dep_rlib(name) -> PathBuf          │
 │   fn extract_sizes(rlib, prefix)             │
 │       -> HashMap<String, u64>                │
 │   (~60 lines total, uses `object` = "0.39")  │
 └──────────────────────────────────────────────┘
```

## Work items

Implementation proceeds in four stages. Stages 1–2 are the probe
infrastructure and can land as a single PR; Stages 3–4 are larger
refactors with public-API surface area, each a separate PR.

- [x] **87.1** Create `nros-sizes-build` build-script utility
- [x] **87.2** Add `nros/src/sizes.rs` with `export_size!` macro + exports
- [x] **87.3** Update `nros-c/build.rs` and `nros-cpp/build.rs` to probe
      `nros`'s rlib; run both hand-math and probe in parallel, assert
      equal, land once green
- [x] **87.4** Delete hand-math; probe is authoritative. All C-side
      hand-math storage upper bounds gone. (C++-side action storage
      hand-math is tracked separately under 87.11.)
- [x] **87.5** `#[repr(C)]` migration for all four `*Internal` shims.
      `ActionServerInternal` finished by adding `#[repr(C)]` to
      `ActionServerRawHandle` (rustc accepts it — fn pointers are
      FFI-safe regardless of trait-object parameters) and replacing
      `Option<ActionServerRawHandle>` with always-present sentinel
      (`ActionServerRawHandle::invalid()` + `INVALID_ENTRY_INDEX`).
- [x] **87.6** Thin-wrapper refactor of `nros-cpp` complete for all
      seven types. ActionServer/ActionClient: action_name / type_name /
      type_hash buffers moved to the C++ classes; storage drops from
      736→72 bytes (ActionServer) and 312→48 bytes (ActionClient).
      `nros_cpp_action_server_register` now takes the names as
      parameters (instead of stashing them in the Rust struct).
- [x] **87.7** Hand-maintained `*_OPAQUE_U64S` macros in
      `nros-c/include/nros/types.h` removed (`NROS_SESSION_OPAQUE_U64S`,
      `NROS_PUBLISHER_OPAQUE_U64S`,
      `NROS_SERVICE_CLIENT_OPAQUE_U64S`, `NROS_GUARD_HANDLE_OPAQUE_U64S`,
      `NROS_LIFECYCLE_CTX_OPAQUE_U64S`). Four module headers
      (`publisher.h`, `init.h`, `guard_condition.h`, `lifecycle.h`)
      switched to probe-derived `NROS_*_SIZE`. `nros-c/include/nros/types.h`
      now transitively includes `nros_config_generated.h` so every
      consumer of any nros C header gets the probed macros automatically.
- [x] **87.8** Verify across every target; update docs.
      Cross-compile verified: `just freertos build` (thumbv7m-none-eabi),
      `just nuttx build` (armv7a-none-eabi), `just threadx_riscv64 build`
      (riscv64gc-unknown-none-elf), and the native posix build (12
      C+C++ examples). Documentation added at
      `book/src/internals/opaque-storage-sizing.md`.

### Audit-driven follow-ups (added 2026-04-22)

A code audit during Phase 87.6 caught additional non-SSoT sizes that
weren't listed in the original work items but belong inside this
phase's scope. Severities reflect drift risk.

- [x] **87.9 (HIGH)** `types.h` opaque-u64s leak fixed. `SESSION_SIZE`
      and `LIFECYCLE_CTX_SIZE` added to `nros::sizes`; four module
      headers (`publisher.h`, `init.h`, `guard_condition.h`,
      `lifecycle.h`) switched to probe-derived macros; the five dead
      `*_OPAQUE_U64S` macros deleted from `types.h`. `types.h` now
      transitively includes `nros_config_generated.h`. Net storage
      shrink on x86_64: ~1 KiB per `nros_support_t` (512 → 0), ~336
      bytes per `nros_publisher_t` (384 → 48), 24 bytes per
      `nros_guard_condition_t`, 64 bytes per
      `nros_lifecycle_state_machine_t`.
- [x] **87.10 (HIGH)** `zpico-platform-shim/build.rs` hard-fails on
      probe failure when zenoh-pico headers exist. Default placeholder
      sizes (16, 8) only used when headers are absent (workspace
      `cargo check` without RMW features) — never silently shipped
      with an incompatible ABI.
- [x] **87.11 (MED)** Layout-mirror trick applied to
      `nros::sizes::CppActionServerLayout` and
      `CppActionClientLayout`. `nros-cpp/build.rs` hand-math gone;
      probed values flow through `NROS_CPP_ACTION_*_STORAGE_SIZE`
      macros. Rust-side byte-equivalence asserts in
      `nros-cpp/src/action.rs` keep the mirrors honest (any field
      change in the real wrappers must be paired with the layout
      update in `nros/src/sizes.rs`).
- [x] **87.12 (LOW)** Zephyr probe-skip path documented with an audit
      hook in `zpico-platform-shim/build.rs`. The `(ptr_size, ptr_size)`
      shortcut stays — a proper Zephyr-aware probe would require
      running inside the west workspace, which is out of scope. The
      comment now flags the specific layout assumption (zenoh-pico
      Zephyr unions = single pointer field) and points reviewers at
      the file to re-verify if it changes.

### 87.1 — `nros-sizes-build` build-script utility

New workspace crate at `packages/core/nros-sizes-build/`. Build-time
library, not published. Provides two functions:

```rust
// packages/core/nros-sizes-build/src/lib.rs
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Locate the compiled rlib for a direct dependency within the current
/// cargo build. Uses `cargo metadata` to find the target directory, then
/// globs `target/<triple>/<profile>/deps/lib<name>-*.rlib` and picks the
/// newest.
pub fn find_dep_rlib(crate_name: &str) -> PathBuf { /* ~25 lines */ }

/// Read all defined symbols starting with `prefix` from an rlib, returning
/// a map of {suffix-after-prefix → ObjectSymbol::size()}. Iterates ar
/// archive members via `object::read::archive::ArchiveFile`; skips
/// `.rmeta` metadata members.
pub fn extract_sizes(rlib: &Path, prefix: &str) -> HashMap<String, u64> {
    /* ~30 lines */
}
```

Dependencies: `object = "0.39"`, `serde_json = "1"` (for cargo metadata
parsing). Both are already transitively present in the workspace via
other build scripts.

**Files**: `packages/core/nros-sizes-build/{Cargo.toml,src/lib.rs,README.md}`.

### 87.2 — `nros/src/sizes.rs`

```rust
//! Single source of truth for FFI storage sizes.
//!
//! Each `export_size!` invocation creates two artefacts:
//!
//! * `pub const FOO_SIZE: usize = core::mem::size_of::<T>();` — for Rust
//!   consumers (including in-crate `const _: () = assert!(...)` checks).
//! * `pub static __NROS_SIZE_FOO: [u8; FOO_SIZE]` — array-sized static
//!   whose *symbol storage size* in the compiled rlib equals `FOO_SIZE`.
//!   `nros-c`/`nros-cpp` build scripts extract these via
//!   `nros_sizes_build::extract_sizes()`.

#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
mod rmw_sizes {
    use nros_node::session::*;

    macro_rules! export_size {
        ($vis:vis $name:ident = $ty:ty) => {
            $vis const $name: usize = core::mem::size_of::<$ty>();
            paste::paste! {
                #[used]
                #[unsafe(no_mangle)]
                pub static [<__NROS_SIZE_ $name>]: [u8; $name] = [0u8; $name];
            }
        };
    }

    export_size!(pub PUBLISHER_SIZE       = RmwPublisher);
    export_size!(pub SUBSCRIBER_SIZE      = RmwSubscriber);
    export_size!(pub SERVICE_CLIENT_SIZE  = RmwServiceClient);
    export_size!(pub SERVICE_SERVER_SIZE  = RmwServiceServer);
    export_size!(pub EXECUTOR_SIZE        = nros_node::Executor);
    export_size!(pub GUARD_CONDITION_SIZE = nros_node::GuardConditionHandle);
}

#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
pub use rmw_sizes::*;
```

**Feature gating**: the statics only exist when an RMW backend is
enabled. Workspace-level `cargo check` (no RMW feature) sees empty
module — same as today's `#[cfg(any(rmw-*))]` pattern in
`opaque_sizes.rs`.

**Files**: `packages/core/nros/src/sizes.rs`,
`packages/core/nros/src/lib.rs` (add `pub mod sizes;`).

### 87.3 — Probe in consumer build scripts

`nros-c/build.rs` and `nros-cpp/build.rs` each add:

```rust
let nros_rlib = nros_sizes_build::find_dep_rlib("nros");
let sizes = nros_sizes_build::extract_sizes(&nros_rlib, "__NROS_SIZE_");

// For transition safety, keep hand-math as `expected_*` and assert:
assert_eq!(sizes["PUBLISHER_SIZE"] as usize, expected_publisher_size,
    "probe / hand-math mismatch — bump hand-math or refresh probe");
```

This mode lands under a feature flag (`verify-probe`) enabled in CI;
once every target reports identical values across a few days of CI, the
hand-math branch is deleted (87.4).

**Files**: `packages/core/nros-c/build.rs`,
`packages/core/nros-cpp/build.rs`.

### 87.4 — Delete hand-math

With the probe proven, remove:

- `target_pointer_bytes()` and all `*_bytes = 4 * ptr_bytes + …`
  arithmetic in `nros-cpp/build.rs` (~90 lines).
- Literal constants (`session_upper`, `entries_upper`, `action_*_bytes`)
  in `nros-c/build.rs` (~30 lines).
- `u64s_for::<T>()` computations in `nros-c/src/opaque_sizes.rs` — the
  `const _: () = assert!(...)` checks stay; the `pub const *_OPAQUE_U64S`
  declarations go (cbindgen wasn't emitting them anyway).

`nros-c/include/nros/types.h`'s hand-written `#define NROS_*_OPAQUE_U64S`
lines come out; the generated header takes over.

**Files**: `packages/core/nros-c/build.rs`,
`packages/core/nros-cpp/build.rs`,
`packages/core/nros-c/src/opaque_sizes.rs`,
`packages/core/nros-c/include/nros/types.h`.

### 87.5 — `#[repr(C)]` for internal FFI shim types

Four structs in `nros-c` currently rely on hand-math because their
sizes end up in C headers via `const _: () = assert!` defence rather
than cbindgen:

| Type | File | Size (today) |
|------|------|--------------|
| `ActionServerInternal` | `nros-c/src/action/server.rs` | ~40 bytes |
| `ActionClientInternal` | `nros-c/src/action/client.rs` | ~16 bytes |
| `ServiceClientInternal` | `nros-c/src/service.rs` | ~24 bytes |
| `ServiceServerInternal` | `nros-c/src/service.rs` | ~16 bytes |

All four contain only C-ABI-compatible fields (function pointers, raw
`*mut c_void`, `i32` indices). The one obstacle is `Option<RawHandle>`
fields — migrate to a plain `i32 = -1` sentinel or an explicit tagged
struct.

After `#[repr(C)]`:

- cbindgen emits matching C struct definitions.
- `nros_action_server_t._internal` becomes a plain inline
  `ActionServerInternal` (no more `uint64_t _internal[OPAQUE_U64S]`).
- No probe, no hand-math, no `const_assert` for these types.

**Files**: `nros-c/src/action/{server,client}.rs`,
`nros-c/src/service.rs`, `nros-c/include/nros/{action,service}.h`,
`nros-c/cbindgen.toml`.

### 87.6 — Thin-wrapper refactor (the big one)

Goal: **both C and C++ consumers hold the same Rust handle**, differing
only in language-native metadata (topic name storage, etc.).

Before:

```rust
// nros-cpp/src/publisher.rs
pub(crate) struct CppPublisher {
    pub handle: RmwPublisher,
    pub topic_name: [u8; 256],
    pub topic_name_len: usize,
}
```

```cpp
// nros-cpp/include/nros/publisher.hpp
class Publisher {
    alignas(8) uint8_t storage_[NROS_CPP_PUBLISHER_STORAGE_SIZE];
    // name is inside storage_ (Rust-side)
};
```

After:

```rust
// No CppPublisher. nros-cpp's FFI operates on RmwPublisher directly.
```

```cpp
class Publisher {
    alignas(8) uint8_t storage_[NROS_PUBLISHER_SIZE];   // same macro as nros-c
    char topic_name_[256];                               // C++-side
    size_t topic_name_len_;                              // C++-side
public:
    const char* topic_name() const { return topic_name_; }  // no FFI hop
};
```

Impact per entity (Publisher, Subscription, ServiceServer, ServiceClient,
ActionServer, ActionClient, GuardCondition — 7 types):

1. Delete the `Cpp*` struct definition (~10 lines/each).
2. Update the FFI functions to take `*mut RmwXxx` (or opaque bytes) and
   operate on the handle directly. ~3 functions/entity.
3. Update the C++ hpp class to declare the metadata as C++ fields.
4. Update the C++ constructor to copy the string into the C++ field
   instead of passing it through FFI.
5. Update `_relocate` FFI to `ptr::read`/`ptr::write` on `RmwXxx`
   directly; the `reregister` path (Executor, ActionServer) stays.

Roughly ~200 lines of diff across 14 files (.rs + .hpp pairs). No
public C++ API change — accessors keep the same signatures.

**Files**: `nros-cpp/src/{publisher,subscription,service,action,guard_condition}.rs`,
`nros-cpp/include/nros/*.hpp`, matching cbindgen output.

### 87.7 — Header cleanup

With 87.4–87.6 in: remove the hand-written `types.h` constants, fold
everything into the generated `nros_config_generated.h`. Keep
`types.h` for non-size material (enum definitions, common typedefs).

**Files**: `nros-c/include/nros/types.h`.

### 87.8 — Cross-target verification + docs

- `cargo build --target thumbv7m-none-eabi --features "rmw-zenoh,…"`
  succeeds without any `STORAGE_SIZE too small` assert.
- Same for `armv7a-nuttx-eabi`, `riscv64gc-unknown-none-elf`,
  `x86_64-unknown-linux-gnu`.
- `cargo nextest run -p nros-tests --test rtos_e2e -E
   'test(Nuttx::lang_2_Lang__C) | test(Nuttx::lang_3_Lang__Cpp)'`
  progresses past build (runtime failures tracked separately in 85.10).
- Book updates: `book/src/internals/opaque-storage.md` (new) or a
  section in the existing C API reference explaining the `export_size!`
  pattern for future maintainers.

## Acceptance criteria

- [ ] `packages/core/nros-c/build.rs` and
      `packages/core/nros-cpp/build.rs` contain **zero** target-specific
      struct-layout math. The only numeric constants they compute are
      `sizes["FOO_SIZE"].div_ceil(8)` / `.next_multiple_of(8)` kind of
      mechanical unit conversions.
- [ ] `packages/core/nros/src/sizes.rs` is the only place in the tree
      where a size-bearing type's identity appears in a `size_of` or
      `[u8; _]` context.
- [ ] `NROS_PUBLISHER_OPAQUE_U64S` (C) and `NROS_PUBLISHER_SIZE` (C++
      thin-wrapper result) derive from the same underlying const
      `nros::sizes::PUBLISHER_SIZE`.
- [ ] `types.h` ships zero hand-written `#define *_OPAQUE_U64S` or
      `*_STORAGE_SIZE` lines.
- [ ] The 3 NuttX C / C++ rtos_e2e cases that fail today with
      "STORAGE_SIZE too small" build past that assertion (runtime
      behaviour out of scope).
- [ ] If cbindgen upstream [#252](https://github.com/mozilla/cbindgen/issues/252)
      ever ships, the architecture requires no restructure — the
      `export_size!` macro's const line is what cbindgen would pick up;
      the probe step degrades to a no-op.

## Design notes

### `find_dep_rlib` mechanics

cargo doesn't expose dep rlib paths to build scripts via any stable env
var. Two reliable strategies:

1. **`cargo metadata --format-version=1 --no-deps`** for
   `target_directory`, then glob
   `{target_directory}/<triple>/<profile>/deps/lib<name>-*.rlib` and
   pick the newest mtime. Used by `cargo-llvm-cov` and similar tools.
2. **`links = "..."` + `cargo:rlib-path=`** — doesn't work here; build
   scripts run before their own crate compiles, so the rlib path isn't
   known to emit.

Phase 87 uses (1). The glob picks up stale rlibs from feature-flag
combinations; newest-mtime selection matches cargo's own incremental
semantics.

Two candidate `deps/` paths are checked: `target/<triple>/<profile>/deps/`
(cross-compile or explicit `--target`) and `target/<profile>/deps/`
(native without `--target`). Whichever exists first wins.

### Why array-sized statics, not value-encoded statics

`pub static FOO: usize = size_of::<T>();` stores the *value* (e.g., 48)
in `.rodata`. `llvm-nm --print-size` would report the symbol's storage
size (8, a usize), not the value. Reading the value requires parsing
`.rodata` bytes via `object`'s section API — workable but an extra step.

`pub static FOO: [u8; size_of::<T>()] = [0; _];` makes the *symbol's
storage size* equal to the type's size. `ObjectSymbol::size()` returns
it directly — no section decoding. This trick is already used by
`zpico-platform-shim/c/size_probe.c` for the C→Rust direction:

```c
const unsigned char __nros_sizeof_net_socket[sizeof(_z_sys_net_socket_t)] = {0};
```

Phase 87 mirrors this pattern on the Rust side.

### Feature-flag forwarding

`RmwPublisher` is `<ConcreteSession as Session>::PublisherHandle`, which
only resolves when exactly one of `rmw-zenoh` / `rmw-xrce` / `rmw-dds`
is active. `nros/src/sizes.rs` mirrors that cfg guard. A no-RMW
workspace check sees no statics; consumers of `nros-c`/`nros-cpp`
always enable an RMW feature anyway.

### Interaction with cbindgen

cbindgen still runs for FFI declarations. It won't see the
`__NROS_SIZE_*` statics (they're not `pub`-exported in the
cbindgen-visible sense — they're symbols in the rlib), nor will it try
to evaluate `size_of` expressions for the size consts (avoided by
keeping the consts' values as array lengths rather than `#define`-able
scalars). This sidesteps cbindgen#252 entirely.

### Out of scope

- **`cxx` adoption**. Removes the size problem by heap-allocating
  everything, but breaks nros-cpp's `no_std` contract. Rejected.
- **`-Z print-type-sizes`**. Nightly-only, unstable output format.
  Rejected.
- **Migrating `zpico-platform-shim` to `object`**. Possible as a
  follow-up phase; out of scope for 87. The shim is 150 lines of
  llvm-nm+sysroot-hunting that would shrink to ~40 with `object`, but
  it works today.

## Notes

- The `export_size!` macro uses `paste` for identifier concatenation
  (`__NROS_SIZE_ ## $name`). `paste` is already a workspace dep.
- The `object` crate has no default-feature caveats relevant here. All
  needed readers (archive, elf, macho, coff) are on by default.
  `read_core` alone would suffice but isn't worth shaving 200 KB off
  the build-script compile.
- `cargo metadata` invocation from build.rs costs ~50 ms per
  consumer crate on a warm cache. Negligible compared to the
  rebuild-of-everything that drops `hand-math`-only.
- The `const _: () = assert!(size_of::<T>() <= STORAGE);` defence-in-depth
  checks in `nros-cpp/src/lib.rs` and `nros-c/src/opaque_sizes.rs` stay.
  Post-phase they become trivially true — the storage size is
  *derived* from `size_of`, so the assertion's failure would mean the
  probe lied. A useful tripwire for probe bugs.
