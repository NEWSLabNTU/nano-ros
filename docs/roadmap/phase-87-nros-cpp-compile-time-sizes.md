# Phase 87: `nros-cpp` Compile-Time Storage-Size Derivation

**Goal**: Replace `nros-cpp/build.rs`'s hand-coded `4 * ptr_bytes + …`
struct-layout math with compile-time-derived sizes obtained from
`core::mem::size_of::<T>()`. The sizes flow from the target Rust
compilation out to a `#define` in `nros_cpp_config_generated.h` via a
probe crate that reflects symbol values from a target-compiled
object file.

**Status**: Not Started
**Priority**: Medium — currently blocks 3 `rtos_e2e` test cases on
NuttX (C + C++), and the same class of undercount will hit FreeRTOS
/ ThreadX once their RMW firmware failures in 85.10 are fixed. Also
closes a latent bug in `nros-c` where `opaque_sizes.rs` thinks it
uses `size_of::<T>()` but cbindgen silently drops the value.
**Depends on**: Phase 85.4 (parametrised RTOS E2E tests exist and
currently surface the undercount as a compile-time assertion
failure). Supersedes [Phase 85.9](./phase-85-test-suite-consolidation.md).

## Overview

### The problem

Every opaque C++ wrapper in `nros-cpp`
(`Publisher`, `Subscription`, `ServiceServer`, `ServiceClient`,
`ActionServer`, `ActionClient`, `GuardCondition`, `Executor`) embeds a
fixed-size byte array for the Rust handle:

```cpp
// packages/core/nros-cpp/include/nros/publisher.hpp:101
alignas(8) uint8_t storage_[NROS_CPP_PUBLISHER_STORAGE_SIZE];
```

The `#define` is emitted at build time by
`packages/core/nros-cpp/build.rs:122-146`, which currently estimates
each handle size with pointer-width math:

```rust
let publisher_bytes = align_up(
    4 * ptr_bytes + name_buf + ptr_bytes + 4 * ptr_bytes,
    8,
);
```

This formula under-counts on 32-bit ARM (`armv7a-nuttx-eabihf`,
`thumbv7m-none-eabi`): zenoh-pico's internal `RmwPublisher` holds more
state than "4 pointers" when expanded field-by-field, so the compile-
time assert at `packages/core/nros-cpp/src/lib.rs:350` trips:

```
evaluation panicked: NROS_CPP_PUBLISHER_STORAGE_SIZE too small for
CppPublisher — bump publisher_bytes in build.rs
```

The failure blocks
`test_rtos_{pubsub,service,action}_e2e::platform_2_Platform__Nuttx::lang_{2_Lang__C,3_Lang__Cpp}`
(3 cases, plus symmetric future blockage on FreeRTOS / ThreadX
variants once those firmware E2Es unblock).

### Why not just bump the formula

The obvious stopgap — bump the `4 * ptr_bytes` to `32 * ptr_bytes` or
some other generous constant — was rejected in Phase 85.4 because it
preserves the underlying problem: `build.rs` continues to guess at
struct layouts by hand. Any future change to `CppPublisher` (added
field, different backend wrapping the handle, Rust compiler layout
changes) can silently undercount again.

The project owner's directive, re-stated in Phase 85.4's commit message:

> we should avoid manual size calculation. Instead, the size should be
> generated in compile time.

### Why a probe crate

Two approaches were investigated and ruled out:

- **cbindgen-evaluated const**. Empirically verified (`/tmp/cbindgen-probe`
  and `/tmp/cbi-probe2`) that cbindgen silently drops any const whose
  value depends on `core::mem::size_of::<T>()`. Tracked upstream at
  [cbindgen#252](https://github.com/mozilla/cbindgen/issues/252) (open
  since 2018), no fix in sight. The `nros-c` crate has the same latent
  bug: `opaque_sizes.rs:32` uses `u64s_for::<RmwPublisher>()` but the
  cbindgen-generated `nros_generated.h` has `#define
  PUBLISHER_OPAQUE_U64S 1` (placeholder branch). It works only because
  `types.h:79` hand-maintains `NROS_PUBLISHER_OPAQUE_U64S 48`.

- **`-Zprint-type-sizes`** is nightly-only with an unstable output
  format; parsing from `build.rs` is brittle.

The remaining feasible approach is a **probe crate**: a sibling crate
compiled for the same target with the same features as `nros-cpp`,
whose `build.rs` invokes `rustc --target $TARGET --emit=obj` on a
generated probe source and extracts `#[used] #[unsafe(no_mangle)] pub
static … = size_of::<T>()` values from the resulting object file via
the `object` crate. The values are forwarded to
`nros-cpp/build.rs` through the standard `links = "…"` + `cargo:KEY=VALUE`
→ `DEP_NROS_CPP_SIZEPROBE_*` cargo metadata channel.

### Why a shared types crate

A naïve probe-crate design duplicates the struct layouts (`CppPublisher`,
`CppSubscription`, …) between `nros-cpp/src/*.rs` and
`nros-cpp-sizeprobe/src/lib.rs`. If the two drift, the probe reports a
stale size; the compile-time assert at `lib.rs:350` catches it
eventually, but only *after* the mismatch trips in CI. That's a
foot-gun that Phase 87 avoids by factoring the struct layouts into a
new **types-only** crate that both `nros-cpp` (wrappers + FFI) and
`nros-cpp-sizeprobe` (sizing) depend on.

### Crate graph (after Phase 87)

```
 ┌────────────────────────────────────────────┐
 │ nros-rmw-zenoh / nros-rmw-xrce / nros-node │  (unchanged)
 └────────────────────┬───────────────────────┘
                      │
                      ▼
 ┌─────────────────────────────────────────┐
 │ nros-cpp-types                          │   NEW — struct defs
 │   CppPublisher, CppSubscription,        │   ONLY, no FFI
 │   CppServiceServer, CppServiceClient,   │   (pub fields, #[repr(C)])
 │   CppActionServer, CppActionClient,     │
 │   CppGuardCondition                     │
 └───────┬──────────────────┬──────────────┘
         │                  │
         ▼                  ▼
 ┌──────────────┐   ┌────────────────────────┐
 │ nros-cpp     │   │ nros-cpp-sizeprobe     │   NEW — compile-time
 │ (wrappers +  │   │   #[used] static       │   size probe, links
 │  FFI, C++    │   │   NROS_CPP_*_SIZE      │   = "nros_cpp_sizeprobe"
 │  headers)    │   │                        │
 └──────┬───────┘   └──────────┬─────────────┘
        │                      │
        └──────────────────────┘
         reads DEP_NROS_CPP_SIZEPROBE_* in build.rs
         → emits #define to nros_cpp_config_generated.h
```

## Work Items

- [ ] 87.1 — Create `packages/core/nros-cpp-types/`
- [ ] 87.2 — Migrate `nros-cpp`'s `Cpp*` struct definitions to
      `nros-cpp-types` (types crate becomes the single source of truth)
- [ ] 87.3 — Create `packages/core/nros-cpp-sizeprobe/` with a
      `build.rs` that invokes `rustc` and parses the object file
- [ ] 87.4 — Rewrite `packages/core/nros-cpp/build.rs` to consume
      sizes from `DEP_NROS_CPP_SIZEPROBE_*` and drop all hand-math
- [ ] 87.5 — Apply the same pattern to `nros-c` (close the latent
      `opaque_sizes.rs` / cbindgen-drop bug in one go)
- [ ] 87.6 — Verify on all target triples + integration tests
- [ ] 87.7 — Doc updates

### 87.1 — Create `packages/core/nros-cpp-types`

- Cargo package with `no_std` + feature gates mirroring `nros-cpp`:
  `rmw-zenoh` / `rmw-xrce` / `rmw-dds` / `rmw-cffi` (forwarded to
  `nros-rmw-zenoh` / `nros-rmw-xrce` / `nros-rmw-dds` / `nros-rmw-cffi`
  respectively), plus the `platform-*` / `ros-*` axes.
- Exports `pub struct CppPublisher { pub handle: RmwPublisher,
  pub topic_name: [u8; MAX_TOPIC_LEN], pub topic_name_len: usize }` and
  the same for `CppSubscription`, `CppServiceServer`,
  `CppServiceClient`, `CppActionServer`, `CppActionClient`,
  `CppGuardCondition`. No `impl` blocks, no FFI — just the layout.
- The fields are `pub` (were `pub(crate)` in `nros-cpp`) so both
  `nros-cpp` and `nros-cpp-sizeprobe` can build them in tests /
  probes without downstream-visibility leakage (the types themselves
  are still only meant for consumption through the C++ header).
- **Files**: `packages/core/nros-cpp-types/{Cargo.toml,src/lib.rs}`.

### 87.2 — Migrate struct definitions out of `nros-cpp`

- Replace the inline `pub(crate) struct CppPublisher { … }` in
  `packages/core/nros-cpp/src/publisher.rs:14` (and siblings) with
  `use nros_cpp_types::CppPublisher;`. Same for
  `subscription.rs:16`, `service.rs:20`, `service.rs:206`,
  `action.rs` (server + client), `guard_condition.rs`.
- `nros-cpp` gains `nros-cpp-types` as a workspace dependency. Feature
  forwarding: `nros-cpp/rmw-zenoh` implies
  `nros-cpp-types/rmw-zenoh` (otherwise the `RmwPublisher` type alias
  inside `nros-cpp-types` has no backend).
- No API change visible to C++ / external callers. Everything behind
  the opaque `storage_` stays byte-identical.
- **Files**: `packages/core/nros-cpp/Cargo.toml`,
  `packages/core/nros-cpp/src/{publisher,subscription,service,action,guard_condition}.rs`.

### 87.3 — Create `packages/core/nros-cpp-sizeprobe`

- Cargo package with `links = "nros_cpp_sizeprobe"`. Depends on
  `nros-cpp-types`.
- `src/lib.rs` is minimal:
  ```rust
  #![no_std]
  use nros_cpp_types::*;
  macro_rules! export_size {
      ($name:ident, $ty:ty) => {
          #[used]
          #[unsafe(no_mangle)]
          pub static $name: usize = core::mem::size_of::<$ty>();
      };
  }
  export_size!(NROS_CPP_PUBLISHER_SIZE, CppPublisher);
  export_size!(NROS_CPP_SUBSCRIPTION_SIZE, CppSubscription);
  export_size!(NROS_CPP_SERVICE_SERVER_SIZE, CppServiceServer);
  export_size!(NROS_CPP_SERVICE_CLIENT_SIZE, CppServiceClient);
  export_size!(NROS_CPP_ACTION_SERVER_SIZE, CppActionServer);
  export_size!(NROS_CPP_ACTION_CLIENT_SIZE, CppActionClient);
  export_size!(NROS_CPP_GUARD_CONDITION_SIZE, CppGuardCondition);
  ```
- `build.rs` does the probe dance:
  1. Wait until the crate's own `cargo build` step has produced the
     `.rlib` for the target (this is implicit — `build.rs` runs
     *after* dep compilation but *before* own `lib.rs` compilation).
  2. **Actually, can't rely on own rlib being built yet**. Instead:
     emit a probe source to `$OUT_DIR/probe.rs` containing the same
     static declarations, invoke
     `rustc --target $TARGET --edition 2024 --crate-type obj
      --extern nros_cpp_types=<path> -O -o $OUT_DIR/probe.o probe.rs`.
  3. The `<path>` for `--extern` is discovered via `cargo metadata
     --format-version=1 --no-deps` from the current workspace (that
     tells us where cargo put the target-compiled `rlib` in
     `target/<target>/release/deps/libnros_cpp_types-HASH.rlib`).
     Alternative: use the `DEP_*` channel from a stub build.rs on
     `nros-cpp-types` itself to forward its own `OUT_DIR`.
  4. Parse `probe.o` with the `object` crate; for each
     `NROS_CPP_*_SIZE` symbol, read its value out of the `.rodata`
     section. Statics of type `usize` with a const initialiser are
     constant-evaluated and end up as raw bytes in `.rodata`.
  5. Emit each as `println!("cargo:{name}={value}")` — cargo translates
     this into `DEP_NROS_CPP_SIZEPROBE_{name}` for dependents.
- **Cross-target caveat**: `rustc` can be invoked for the target even
  on a host that can't execute the target (we only need the object
  file, not an executable). Confirmed workable for ARM / RISC-V
  cross-compilation.
- **Files**: `packages/core/nros-cpp-sizeprobe/{Cargo.toml,src/lib.rs,build.rs}`.

### 87.4 — Rewrite `packages/core/nros-cpp/build.rs`

- Delete `target_pointer_bytes()`, all the `publisher_bytes = …`
  hand-math (lines 60–146).
- Replace with:
  ```rust
  let publisher_storage = env::var("DEP_NROS_CPP_SIZEPROBE_NROS_CPP_PUBLISHER_SIZE")
      .expect("DEP_NROS_CPP_SIZEPROBE_NROS_CPP_PUBLISHER_SIZE not set — is nros-cpp-sizeprobe a direct dependency?")
      .parse::<usize>()
      .expect("invalid usize");
  // … similar for subscription, service, action_server, action_client,
  //       guard_condition, executor.
  ```
- The rest of `build.rs` (writing `nros_cpp_ffi_config.rs` + the C
  header) stays byte-identical except the `format!` args now read
  from the parsed env values.
- **Files**: `packages/core/nros-cpp/build.rs`.

### 87.5 — Apply the same pattern to `nros-c`

- `packages/core/nros-c/src/opaque_sizes.rs:32` has
  `pub const PUBLISHER_OPAQUE_U64S: usize =
   u64s_for::<nros::internals::RmwPublisher>();` — cbindgen drops
  this, so `include/nros/nros_generated.h:85` shows
  `#define PUBLISHER_OPAQUE_U64S 1` (the no-RMW placeholder).
  `include/nros/types.h:79` hand-maintains `#define
  NROS_PUBLISHER_OPAQUE_U64S 48`; the two names diverge silently.
- Fix: re-use the same sizeprobe crate (or a sibling `nros-c-sizeprobe`
  if type-graph clash prevents sharing) to emit the RmwPublisher /
  RmwSession / RmwServiceClient sizes. `nros-c/build.rs` reads them
  via `DEP_*` and writes `types.h` instead of hand-maintaining it.
- Alternatively, if `nros-cpp-types` already has the required type
  aliases (`RmwPublisher`, `RmwSession`, `RmwServiceClient`), extend
  `nros-cpp-sizeprobe` with those additional statics. Most likely
  cleanest — the RMW types are shared across both C and C++ APIs.
- **Files**: `packages/core/nros-c/build.rs`,
  `packages/core/nros-c/include/nros/types.h` (becomes generated),
  `packages/core/nros-c/src/opaque_sizes.rs` (replace with the same
  `DEP_*` read-through pattern, or keep the asserts and drop the
  now-redundant `u64s_for::<T>()` computation).

### 87.6 — Verification

- `cargo build -p nros-cpp --target armv7a-nuttx-eabihf
  --features "rmw-zenoh,platform-nuttx,ros-humble"` succeeds without
  the `NROS_CPP_PUBLISHER_STORAGE_SIZE too small` assert firing.
- Same for `thumbv7m-none-eabi` (FreeRTOS), `riscv64gc-unknown-none-elf`
  (ThreadX RISC-V), and `x86_64-unknown-linux-gnu` (native).
- `cargo nextest run --test rtos_e2e
   -E 'test(platform_2_Platform__Nuttx::lang_{2_Lang__C,3_Lang__Cpp})'`
  progresses past the build step (may still hit the Phase 85.10 RV64
  connect failure downstream; that's out of scope for 87).
- `diff` before / after of each target's `nros_cpp_config_generated.h`:
  the sizes should be *smaller or equal* on 32-bit targets (the old
  formula over-padded for some fields while under-padding the handle)
  and *similar* on 64-bit targets.

### 87.7 — Doc updates

- `book/src/internals/nros-cpp-opaque-storage.md` (new) or a section
  in the existing C++ API reference explaining the sizeprobe design
  for future maintainers.
- Comment block at the top of `packages/core/nros-cpp-sizeprobe/build.rs`
  enumerating the `--extern` discovery strategy and citing the cargo
  `links` docs + the `object` crate.

## Design Notes

### Finding `--extern` paths from `build.rs`

The hardest part of 87.3 is step 3: the probe's `build.rs` needs to
tell `rustc` where to find `libnros_cpp_types-*.rlib`. Three viable
strategies ranked by robustness:

1. **`cargo metadata --format-version=1 --no-deps` + path glob**. Parse
   the JSON, find `nros-cpp-types`, read `manifest_path`, then derive
   `target/<target>/<profile>/deps/libnros_cpp_types-*.rlib`. The
   hash suffix needs a glob-match. Brittle across cargo releases but
   currently the standard approach used by `cargo-llvm-cov` and
   similar tools.

2. **`nros-cpp-types` emits its own `OUT_DIR` via `links`**. Add
   `links = "nros_cpp_types"` to `nros-cpp-types/Cargo.toml` and have
   its build.rs emit `cargo:out-dir=$OUT_DIR`. Dependents (including
   the sizeprobe's `build.rs`) read `DEP_NROS_CPP_TYPES_OUT_DIR`.
   `OUT_DIR` is next to (but not identical to) the rlib dir; a fixup
   is needed. Less brittle than glob-matching.

3. **Invoke `cargo rustc -p nros-cpp-types --target $T -- --print
    file-names`**. Canonical but involves a nested cargo, which
   cargo itself discourages. Fallback only.

Recommend (1) as the primary path, (2) as a reliability backstop if
(1) breaks. The probe crate's build.rs should log the discovered path
to `stderr` so drift is debuggable.

### `object` crate symbol extraction

The probe file exports `#[used] #[unsafe(no_mangle)] pub static
NROS_CPP_PUBLISHER_SIZE: usize = core::mem::size_of::<CppPublisher>();`.
rustc constant-evaluates the `size_of::<T>()` call at compile time for
the target, so the emitted `.rodata` entry for the symbol contains the
raw 4- or 8-byte little-endian number. Algorithm:

```rust
use object::{Object, ObjectSection, ObjectSymbol};
let data = std::fs::read(&probe_obj)?;
let file = object::File::parse(&*data)?;
for sym in file.symbols() {
    let Ok(name) = sym.name() else { continue };
    if !name.starts_with("NROS_CPP_") { continue }
    let Some(sect_idx) = sym.section_index() else { continue };
    let sect = file.section_by_index(sect_idx)?;
    let bytes = sect.data()?;
    let off = sym.address() - sect.address();
    // target endianness + pointer width from file.architecture()
    let value = read_usize_le(&bytes[off as usize..], ptr_bytes);
    println!("cargo:{}={value}", name);
}
```

### Feature-flag forwarding

Because `RmwPublisher` is `<ConcreteSession as
Session>::PublisherHandle`, it only exists when a backend feature is
enabled. `nros-cpp-types` and `nros-cpp-sizeprobe` must forward the
same mutually-exclusive RMW feature axis as `nros-cpp`:
`rmw-zenoh` / `rmw-xrce` / `rmw-dds` / `rmw-cffi`. The compile-time
check `#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature
= "rmw-dds", feature = "rmw-cffi"))]` that currently guards `lib.rs`
also guards `nros-cpp-types::*`. A build with no RMW feature produces
placeholder sizes (or outright omits the symbols) — same semantics as
today's no-RMW build.

### Cost estimate

- 87.1 + 87.2: ~2–4h (mechanical move; `pub(crate)` → `pub` and an
  import statement in each consumer).
- 87.3 + 87.4: ~6–10h (the probe is the first use of object-file
  parsing in this repo, so some debugging is likely).
- 87.5: ~3–4h (mirror of the nros-c work once the pattern is proven).
- 87.6 + 87.7: ~2–3h.

Total: ~2–3 focused days.

## Acceptance Criteria

- [ ] `packages/core/nros-cpp/build.rs` contains **zero**
      target-specific struct-layout math. The only build-time
      constants it reads are config values from `nros-node`'s
      `links` metadata (`DEP_NROS_NODE_RX_BUF_SIZE`, `MAX_CBS`,
      `ARENA_SIZE`) and size values from `nros-cpp-sizeprobe`'s
      `DEP_NROS_CPP_SIZEPROBE_*`.
- [ ] `cargo build -p nros-cpp --target armv7a-nuttx-eabihf
       --features …` succeeds, producing a
      `nros_cpp_config_generated.h` whose `NROS_CPP_PUBLISHER_STORAGE_SIZE`
      is exactly `size_of::<CppPublisher>()` rounded up to 8.
- [ ] The `const _: () = assert!(size_of::<T>() <= STORAGE_BYTES)` in
      `packages/core/nros-cpp/src/lib.rs:349-375` remains in place
      as a defence-in-depth check; it must pass trivially (the bound
      is now tight).
- [ ] No code duplication: `CppPublisher` / `CppSubscription` / …
      appear exactly once (in `nros-cpp-types`), imported by both
      `nros-cpp` and `nros-cpp-sizeprobe`.
- [ ] `nros-c` has its latent cbindgen-drop bug closed in the same
      PR, or explicitly deferred to a named follow-up phase.
- [ ] The 3 NuttX C / C++ `rtos_e2e` cases that currently fail with
      the compile-time-assert message build past the assert. (They
      may still fail at runtime for reasons outside 87's scope —
      that belongs to Phase 85.10 and Phase 69.x follow-ups.)

## Notes

- **Why not use `cxx`?** The `cxx` bridge crate moves ownership of
  Rust types behind pointers — no inline storage in the C++ class.
  That removes the size-derivation problem entirely, but requires
  an allocator on every target. nros-cpp's `no_std` contract
  explicitly rules out heap allocation for these handles (several
  downstream platforms — bare-metal, some FreeRTOS builds — have no
  `alloc`). Keeping inline storage is the design constraint; Phase 87
  serves that constraint.

- **Why not use `-Zprint-type-sizes`?** The flag is nightly-only, the
  output format is not stable, and stable-Rust builds of nros-cpp are
  an intentional goal (`armv7a-nuttx-eabihf` is the only target that
  needs nightly, and it needs nightly for `-Z build-std`, not for
  type-size inspection). Parsing `-Zprint-type-sizes` from `build.rs`
  also couples the whole crate to nightly.

- **Interaction with Phase 85.9**. Phase 85.9 is superseded by this
  phase. The "stopgap bump" path called out in 85.9 is explicitly
  rejected; the proper fix lives here. Mark 85.9 as
  `superseded-by: phase-87` rather than implementing the bump.

- **cbindgen upstream fix**. An alternative that removes the whole
  probe infrastructure would be cbindgen gaining const-eval support
  for `size_of`. The relevant upstream issue is
  [cbindgen#252](https://github.com/mozilla/cbindgen/issues/252). If
  it ever lands, Phase 87 becomes obsolete — the probe crate could
  be deleted and `opaque_sizes.rs` would directly drive the C header.
  Not holding breath; opened 2018.
