# Phase 119 — C++ Executor Storage Corruption + Timer First-Tick

**Goal:** Fix the pre-existing test failures in `test_native_*::Cpp`, `test_cpp_*`, `test_c_xrce_*`, and `test_xrce_action_*` caused by C++ executor opaque-storage overflow and first-spin-tick delta starvation.
**Status:** 119.1 landed (max-merge across cmake variants). Empirically: 5 of 6 originally-failing tests now PASS; 1 (`test_xrce_action_fibonacci`) still fails for an unrelated XRCE-protocol reason. 1 regression introduced: `test_zephyr_cpp_action_server_to_client_e2e` (was passing pre-119) now fails because the merged Zephyr-variant header allocates 4× larger service-client buffer (cyclonedds outlier dominates the max). Followups in 119.2 (variant-specific headers).
**Priority:** High — blocks `just test-all` from green; all C/C++ side examples affected.
**Depends on:** Phase 87 (size probe), Phase 118.E (probe race-hardening).

## Overview

Six tests fail on main (verified pre-118.E via `c77cf84f` checkout):

| Test | Symptom |
|---|---|
| `test_native_talker_listener_communication::Cpp` | C++ listener receives 0 messages from C++ talker. |
| `test_native_service_communication::Cpp` | C++ service never responds. |
| `test_cpp_action_communication` | Goal REJECTED (ret=-2) at action server. |
| `test_cpp_rust_pubsub_interop` | Rust listener receives 0 from C++ talker. |
| `test_c_xrce_talker_listener_communication` | C XRCE talker→listener fails. |
| `test_xrce_action_fibonacci` | XRCE action protocol failure. |

Common pattern: **C/C++ side fails to send/dispatch**. Rust-only paths pass.

## Architecture

### Finding 1 — Memory corruption in `nros::Node::GlobalStorageHolder`

Empirically observed during debug instrumentation:

```
[TALK] iter=0 g_running=1 nros::ok()=1     # before first spin_once
[DBG] spin_once entry timeout_ms=100
[DBG] ... (one full cycle)
[DBG] spin_once returning
[TALK] iter=1 g_running=1 nros::ok()=68    # ← bool is now 68
```

The C++ executor's static storage layout
(`packages/core/nros-cpp/include/nros/node.hpp:316–329`):

```cpp
template <int = 0> struct GlobalStorageHolder {
    alignas(8) static uint8_t storage[NROS_CPP_EXECUTOR_STORAGE_SIZE];  // 17432
    static bool initialized;                                             // overflows here
};
```

`nros_cpp_init` does `core::ptr::write(storage as *mut CppContext, ctx)` —
which corrupts `initialized` if `size_of::<CppContext>() > NROS_CPP_EXECUTOR_STORAGE_SIZE`.
Byte 0x44 (= 68 = 'D') is the first byte past the storage end, written from a
field of the Rust `CppContext` struct.

The compile-time `const_assert!` at `packages/core/nros-cpp/src/lib.rs:438`
guards against this:

```rust
const _: () = assert!(
    core::mem::size_of::<CppContext>() <= CPP_EXECUTOR_OPAQUE_U64S * core::mem::size_of::<u64>(),
    "CPP_EXECUTOR_OPAQUE_U64S too small for CppContext — ..."
);
```

…but only when nros-cpp itself is compiled. **The header value
`NROS_CPP_EXECUTOR_STORAGE_SIZE` is emitted from the *probe* result, while the
linked `libnros_cpp_zenoh.a` was compiled with the *actual* feature set.** If
those diverge, the const_assert passes inside the rlib but the public header
exposes a wrong size to user code.

Hypothetical drivers:

- nros-c's probe enables `param-services`/`lifecycle-services` for its
  install variant; nros-cpp's probe doesn't, but the cmake-built libnros
  for nros-cpp pulled them in via a workspace-wide feature unification.
- Probe target dir contamination across cargo invocations (mitigated in
  Phase 118.E.2 via `rustc_version_slug`, but feature-set drift is
  separate).
- `--no-default-features` propagation: the nested probe explicitly disables
  defaults, but the outer cmake build may enable `std` via dependency
  closure, changing `Executor` layout.

### Finding 2 — `delta_ms=0` on first spin_once

The first `Executor::spin_once` call observes `elapsed=66µs` between
`last_spin_end` (seeded at construction) and the current `Instant::now()`.

```
[DBG] last_spin_end=Instant { tv_sec: 7868165, tv_nsec: 888539852 }
           now=Instant { tv_sec: 7868165, tv_nsec: 888607680 }
           elapsed=66.575µs
```

Sub-millisecond → `delta_ms=0`. Timer's `elapsed_ms` doesn't advance,
callback doesn't fire. C++ talker uses `create_timer` + `spin_once(100)`
loop; Rust talker uses `thread::sleep(1s)` + manual `publish`, so the
Rust path bypasses this issue entirely.

But — the first call should have `delta_ms` ≥ several milliseconds of
init time (publisher creation, locator parse, etc.). Why 66µs?

`last_spin_end` is initialized inside `Executor::from_session` /
`Self::new`, which runs **at the very end of `Executor::open`**. So
`last_spin_end` ≈ "moment Executor::open returns". The 66µs is the
time between `Executor::open` returning and the first `spin_once`
entry — which IS short, because the C++ wrapper immediately registers
the publisher + timer and enters the spin loop with no user-side delay.

So the design intent ("credit time spent before first spin to timers")
is broken on the very first tick: the seed point is after most of the
construction work, not before.

### Finding 3 — Only one spin_once per process despite loop

Even after the first spin_once returns, the C++ talker prints
`iter=1, 2, 3, …` (loop iterating) but no debug output from spin_once.

Hypothesis A: memory corruption from Finding 1 also corrupts the
function pointers in `entries` or the executor's vtable, so subsequent
`nros_cpp_spin_once` calls return early or jump to wrong code without
hitting the debug print. Consistent with `nros::ok() == 68` (corrupted
bool) — adjacent memory got scribbled.

Hypothesis B: instrumentation drops on a code path that fails the cfg
gate. (Unlikely; `#[cfg(feature = "std")]` is satisfied.)

Likely (A). Finding 1 fix dissolves this symptom.

## Work Items

### 119.1 — Max-merge across cmake variants — **DONE**

- **Files:** `packages/core/nros-sizes-build/src/lib.rs`, `packages/core/nros-c/build.rs`, `packages/core/nros-cpp/build.rs`.
- [x] Added `merge_header_max_values(header_path, header_prefix, new_values)` helper to `nros-sizes-build`. Each consumer build.rs calls it before writing the generated header: takes max(probed value, existing header value) per key.
- [x] nros-c and nros-cpp build.rs call the helper with prefix `"NROS_"`. Result: subsequent cmake builds (zenoh, xrce, dds, cyclonedds, freertos, threadx-linux, threadx-riscv64) each emit their target-specific sizes, and the SHARED package-source header converges to the MAX across all variants.
- [x] All variants now fit safely in the resulting storage upper bound. Verified via `nm --print-size build/install/lib/libnros_cpp_*.a | grep __NROS_SIZE_EXECUTOR_SIZE`: every variant's actual Rust Executor size <= header-declared storage.

**Empirical results:**

| Test | Pre-119 | Post-119.1 |
|---|---|---|
| `test_native_talker_listener_communication::Cpp` | FAIL | **PASS** |
| `test_native_service_communication::Cpp` | FAIL | **PASS** |
| `test_cpp_action_communication` | FAIL | **PASS** |
| `test_cpp_rust_pubsub_interop` | FAIL | **PASS** |
| `test_c_xrce_talker_listener_communication` | FAIL | **PASS** |
| `test_xrce_action_fibonacci` | FAIL | FAIL (XRCE protocol, unrelated) |
| `test_zephyr_cpp_action_server_to_client_e2e` | PASS | **REGRESS** (see 119.2) |

`just test-all` count: 13 failed → 9 failed (net −4).

### 119.2 — Variant-specific generated headers — **TODO**

**Why:** the max-merge in 119.1 is a safe upper bound that fits every variant, but it forces every variant to allocate buffers sized for the LARGEST variant (cyclonedds dominates `NROS_SERVICE_CLIENT_SIZE` at 4632 bytes vs ~568 for the others). On memory-constrained Zephyr targets the extra .bss bloat is rejected — e.g. `test_zephyr_cpp_action_server_to_client_e2e` regressed (action goal returns `send_goal: -1` likely from socket-allocation failure under increased static-memory pressure).

**Architecture:**

- Each cmake build (`<rmw>_<platform>`) gets its OWN per-variant header at a variant-specific path. Either:
  - **Option A**: `packages/core/nros-cpp/include/nros/nros_cpp_config_generated_<variant>.h` (variant-suffixed siblings in the same dir; a stub `nros_cpp_config_generated.h` selects via `#ifdef NROS_RMW_* && NROS_PLATFORM_*`).
  - **Option B**: install to `build/install/<variant>/include/nros/...`; CMake's exported `INTERFACE_INCLUDE_DIRECTORIES` lists the variant's dir before the shared dir.
- Option A is less invasive (no CMakeLists.txt changes). Option B is more idiomatic to CMake/find_package conventions.

**Files (Option A):**

- `packages/core/nros-cpp/build.rs`: derive variant string from cargo features (`rmw_*` + `platform_*`); write to a variant-named file; stub `nros_cpp_config_generated.h` includes the right variant header via `#ifdef`.
- `packages/core/nros-c/build.rs`: same pattern.
- Update Doxyfile excludes if needed.

### 119.3 — Add a guard byte after `storage_` in `GlobalStorageHolder`

- **Files:** `packages/core/nros-cpp/include/nros/node.hpp`.
- Inject a `static constexpr uint64_t GUARD = 0xDEADBEEFCAFEBABE` right after `storage[]` (with `alignas(8)` to keep predictable offset). After `nros_cpp_init` returns, verify guard intact.
- Emit `cargo:warning`/`fprintf(stderr, …)` + fail-fast in debug builds. The Rust-side test asserts the guard, surfacing the corruption explicitly.

### 119.3 — Audit feature drift between probe and linked nros

- **Files:** `packages/core/nros-sizes-build/src/lib.rs`, `packages/core/nros-cpp/build.rs`.
- Capture the feature set that the nested cargo invocation actually used and emit it as a `cargo:rustc-env=NROS_CPP_PROBE_FEATURES=<comma-list>`.
- In `nros_cpp_init`, compare against a Rust-side `compile_time_features!()` macro that lists the nros features actually compiled in. Bail with an error if drift detected.
- Document the resolver behavior: `cargo metadata --no-deps` lists declared features, NOT resolved-active features. Use `--filter-platform=$TARGET` to get the resolve graph, then walk it to find `nros`'s active features in the context of the consumer's build.

### 119.4 — Reseed `last_spin_end` on first spin_once

- **Files:** `packages/core/nros-node/src/executor/spin.rs`.
- Add a `first_spin: bool` field. On the first `spin_once` call, prime `last_spin_end` to `spin_start - timeout` so the first cycle credits its requested timeout to timers. This matches user intent for the C++ `create_timer + spin_once` pattern; subsequent cycles use wall-clock delta as today.
- Update the Phase 110 `test_spin_once_does_not_credit_timeout_to_timer_delta` test to acknowledge the new first-tick behavior, or scope the change to the first call only.

### 119.5 — Per-test verification

- Re-run each of the six failing tests after 119.1–.4.
- Track per-test before/after status in this doc.

### 119.6 — Backstop assertion

- **Files:** `packages/core/nros-cpp/src/lib.rs`.
- Promote the current `const_assert!` to ALSO assert against the
  *header-exported* `NROS_CPP_EXECUTOR_STORAGE_SIZE` macro (sourced via
  `include_str!` of the generated header at build time, parsed for the
  define). If `size_of::<CppContext>() > <header value>` the rlib won't
  link, surfacing the drift at the SAME compile step that emits the
  header.

## Acceptance

- [ ] 119.1 lands; runtime size check rejects writes that would overflow.
- [ ] 119.2 lands; guard-byte tripwire catches the corruption in a regression test.
- [ ] 119.3 lands; probe/linked feature-set comparison emits a hard error on drift.
- [ ] 119.4 lands; first-tick timer fires within one period under the
  C++ `create_timer + spin_once` pattern.
- [ ] All six listed tests pass under `just test-all`.
- [ ] `just verify-size-probe` still green.
- [ ] No regressions: `just test-unit`, `just build-all` clean.

## Notes

### Why `nros::ok() == 68`?

68 = 0x44 = ASCII 'D'. The first byte of the corrupted region after
`storage_[]` came from the Rust `CppContext` struct. The first field
of `CppContext` is `executor: Executor`; `Executor`'s first field is
the `SessionStore` enum. The discriminant byte (depending on variant)
or first field bytes likely produced 0x44. Specifically the variant
tag for `SessionStore::Owned(_)` may be encoded with discriminant
overlapping with the byte that ends up at the overflow offset.

### Why does the Rust talker pass?

`examples/native/rust/zenoh/talker/src/main.rs` uses
`std::thread::sleep(1s)` + manual `publisher.publish()`, never calls
`add_timer`. Bypasses both the timer-fire and the storage-corruption
paths (Rust `Executor` is a stack/heap struct managed by Rust, not
opaque storage). So Finding 1 is C++-FFI-specific.

### Relationship to Phase 118.E

Phase 118.E's isolated probe path is the most likely culprit for
Finding 1 (feature drift between probe and link). Reverting to
filesystem-mode probe via `NROS_SIZES_PROBE_MODE=filesystem` and
rebuilding does NOT fix the failure (verified during investigation),
so the drift exists pre-118.E too — but isolated mode may amplify it
in some configurations. 119.3's audit will quantify.
