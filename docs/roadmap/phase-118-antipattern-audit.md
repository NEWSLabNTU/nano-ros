# Phase 118 — Antipattern Audit

**Goal:** Catalog antipatterns surfaced by codebase audit. Prioritize for follow-up phases.
**Status:** Audit complete. Remediation phases TBD.
**Priority:** Medium (technical debt; no functional regressions blocking).
**Depends on:** —

## Overview

Audit covered seven categories: magic numbers / hardcoded values, size hand-math,
C/C++ wrapper thin-ness vs Rust, duplicate types/functions, hand-crafted bindgen
mirrors, build-script self-containment, plus general code-quality antipatterns
(unsafe-without-SAFETY, dead code, re-export sprawl, etc.).

## Architecture

Each category below lists concrete findings with `file:line` anchors, severity,
and a fix sketch. Severity:

- **High** — breaks if changed without coordination; correctness risk.
- **Medium** — duplicate-source-of-truth; consistency drift over time.
- **Low** — stylistic; low maintenance cost today, cleanup-class.

## Work Items

### 118.A — Magic numbers / hardcoded values / paths

#### A.1 (High) Centralize per-platform port scheme
- `packages/testing/nros-tests/src/platform.rs:125-245` — `BAREMETAL=7450`,
  `FREERTOS=7451`, `NUTTX=7452`, `THREADX=7453`, `ESP32=7454`,
  `THREADX_LINUX=7455`, `ZEPHYR=7456` scattered across enum arms; no const named
  for the base port `7450` or stride `+1`.
- `packages/testing/nros-tests/src/platform.rs:44-46` — variant offsets
  `Pubsub=0, Service=10, Action=20` only documented in comments.
- **Fix:** `packages/core/nros-const/` (new) holds `BASE_PORT=7450`,
  `VARIANT_OFFSET_PUBSUB=0`, `VARIANT_OFFSET_SERVICE=10`,
  `VARIANT_OFFSET_ACTION=20`; platform enum derives port via
  `BASE_PORT + plat as u16 + variant_offset`.

#### A.2 (High) Default locator strings duplicated across crates
- `cmake/NanoRosConfig.cmake:36`, `nros-c/cmake/NanoRosReadConfig.cmake:47`,
  `nros-node/src/executor/types.rs`, `nros-cpp/src/lib.rs`, plus 10+
  `packages/boards/*/src/config.rs` files — all hardcode
  `"tcp/127.0.0.1:7447"` or `"tcp/192.0.3.1:7448"`.
- **Fix:** single `nros-const` constant; cmake reads via `read_config()` env-var
  fallback chain.

#### A.3 (High) Buffer sizes inline in nros-c
- `nros-c/src/action/client.rs:517` — `static mut BLK_RESULT_BUF: [u8; 1024]`,
  size repeated at line 531.
- `nros-c/src/service.rs:980` — `static mut BLK_BUF: [u8; 4096]`, repeated at
  line 988.
- **Fix:** module-level `const ACTION_RESULT_BLK_BUF: usize = 1024;` etc.;
  reference in array decl + `.min()` site.

#### A.4 (Medium) Service timeout duplicated
- `nros-c/src/service.rs:381` defines `NROS_DEFAULT_SERVICE_TIMEOUT_MS = 5000`.
- `nros-c/src/service.rs:1527` repeats `client._internal.timeout_ms = 5000`
  literal.
- `packages/core/nros-c/src/action/client.rs:284` + `nros-c/src/service.rs:695`
  both define `const PROBE_TIMEOUT_MS: u32 = 1000` independently.
- **Fix:** single `const PROBE_TIMEOUT_MS` in nros-c root module.

#### A.5 (Medium) Network config defaults in 10+ board crates
- `packages/boards/nros-board-nuttx-qemu-arm/src/config.rs:64-66`,
  `mps2-an385-freertos`, `esp32-qemu`, `threadx-qemu-riscv64`, etc. — each
  hardcodes `[192, 0, 3, 10]` IP, `[192, 0, 3, 1]` gateway,
  `02:00:00:00:00:01` MAC.
- **Fix:** `nros-const::default_network()` returning shared struct.

#### A.6 (Medium) Arena hand-math in nros-node/build.rs
- `packages/core/nros-node/build.rs:40` —
  `(max_cbs * (rx_buf_size * 3 + 512) + 2048).max(8192)`. Magic multiplier 3,
  offsets 512, 2048, min 8192 unexplained.
- **Fix:** named consts (`PER_HANDLE_BUFFER_FACTOR`, `EXECUTOR_OVERHEAD`,
  `MIN_ARENA_BYTES`) + comment explaining each.

#### A.7 (Low) Action goal/result sizes
- `nros-c/src/action/client.rs:903-904` + `action/server.rs` — `result_size_max:
  264` and `goal_size: 8` repeated 5+ times.
- **Fix:** module-local consts.

### 118.B — Size hand-math

Status: **controlled** — every hand-math has a sibling probe + compile-time
assert. Phase 87.4 / 87.6 mark drop-points.

#### B.1 Executor opaque storage (nros-c + nros-cpp)
- `packages/core/nros-c/build.rs:83-87` — `session_upper=512` +
  `entries_upper=max_cbs*80` + `overhead=1536`.
- `packages/core/nros-cpp/build.rs:70-75` — identical formula.
- Probes at `nros-c/build.rs:124+` and `nros-cpp/build.rs:155+`. Asserts at
  `:144-150` / `:165-171`.
- **Fix (Phase 87.6):** drop hand-math; use probed `EXECUTOR_SIZE` directly.
  Risks: probe machinery must work for every `(rmw, platform)` combo before
  drop.

#### B.2 Action server/client fallback (nros-cpp)
- `nros-cpp/build.rs:101-104` — pointer-width fallback used when LTO bitcode
  rlib returns 0 from probe (`action_server_fallback = ptr_bytes * 9`).
- Phase 77.23/77.24 documented.
- **Fix:** make probe machinery LTO-aware (read bitcode types directly, or run
  probe pre-LTO).

#### B.3 div-ceil conversions (NOT hand-math)
- `nros-c/build.rs:159-165` — `probe.div_ceil(8)` for u64 alignment. Derives
  from probe; OK.

### 118.C — C/C++ thin-wrapper violations

C and C++ layers should delegate to `nros-node`. Audit found 15 violations in 2
clusters: blocking/timing logic, and per-entity state mirroring.

#### C.1 (High) Manual blocking + timeout loops in nros-c
- `nros-c/src/service.rs:947-1068` — `nros_client_call()` reimplements full
  blocking spin: wall-clock budgeting, custom callback into `BLK_*` statics,
  state machine. Should delegate to `Promise::wait()`.
- `nros-c/src/service.rs:655-754` — `nros_client_wait_for_service()`
  reimplements probe-and-retry with nested loops. Should delegate to
  `Client::wait_for_service()`.
- `nros-c/src/parameter.rs` (~1400 lines) — request/response sequence-id
  matching + buffering + state machine duplicates what
  `nros-node`'s parameter client should expose as Promise/Future.
- **Fix:** push blocking primitives into nros-node; nros-c calls them.

#### C.2 (High) Timing loops in nros-cpp headers
- `nros-cpp/include/nros/stream.hpp:69-92` — `Stream<T>::wait_next()`
  iteration-count timer (`elapsed += step`) instead of wall-clock; breaks on
  early-wake (Phase 89.2 fixed Future, missed Stream).
- `nros-cpp/include/nros/executor.hpp:125-137` — same iteration-count pattern in
  `Executor::spin(uint32_t duration_ms)`.
- `nros-cpp/include/nros/future.hpp:84-109` — Future correctly does wall-clock.
- **Fix:** unify via single `nros_cpp_wait_until_or_predicate(ms, fn, ctx)` FFI
  primitive; both Stream and Executor::spin call it.

#### C.3 (Medium) Periodic-spin timing in nros-c
- `nros-c/src/executor.rs:1202-1239` — `nros_executor_spin_period()` maintains
  invocation timestamp, accumulates period, sleeps remainder.
- `nros-c/src/executor.rs:1247-1278` — `nros_executor_spin_one_period()`
  duplicates the elapsed-and-sleep pattern.
- **Fix:** nros-node `Executor::spin_periodic()` returns next-tick time;
  C just calls it.

#### C.4 (Medium) Lazy stream-binding in nros-cpp
- `nros-cpp/include/nros/subscription.hpp:136-141` —
  `Subscription<M>::stream()` lazily binds stream state on first call.
- `nros-cpp/include/nros/action_client.hpp:243-250` — same for
  `feedback_stream()`.
- **Fix:** bind during `create_subscription()` / `create_action_client()`.

#### C.5 (Medium) Custom Waker vtable in C
- `nros-c/src/service.rs:30-49` — `atomic_bool_waker()` builds a `RawWakerVTable`
  to wake on `AtomicBool` flip. This is task infrastructure that should live in
  nros-node, exposed as `nros_node::register_atomic_bool_waker()`.

#### C.6 (Medium) Move-construct memcpy in nros-cpp
- `nros-cpp/include/nros/executor.hpp:167-188` — manual byte-for-byte
  `storage_[i] = other.storage_[i]` instead of calling
  `nros_cpp_executor_relocate()`. Breaks if executor body holds self-pointers.
- `publisher.hpp:104-126`, `subscription.hpp:162`, `client.hpp:107`,
  `service.hpp:108`, `action_client.hpp:346` — same pattern + topic-name
  `memcpy` (topic name stored both C++-side and runtime-side).
- **Fix:** all wrapper move ctors call the corresponding `*_relocate()` FFI;
  topic name stored runtime-side only, accessed via getter.

### 118.D — Hand-crafted mirror types vs cbindgen

#### D.1 (Medium) Systematic duplication: 16 nros-cpp .hpp files redeclare FFI
Each of these has an `extern "C" { ... }` block redeclaring symbols already
present in `packages/core/nros-cpp/include/nros_cpp_ffi.h` (cbindgen output,
1228 lines):

```
nros-cpp/include/nros/executor.hpp        :20-31
nros-cpp/include/nros/publisher.hpp       :21-48
nros-cpp/include/nros/service.hpp         :20-29
nros-cpp/include/nros/timer.hpp           :24-36
nros-cpp/include/nros/action_server.hpp   :21-45
nros-cpp/include/nros/action_client.hpp   :23-48
nros-cpp/include/nros/subscription.hpp    :22-55
nros-cpp/include/nros/guard_condition.hpp :25-34
nros-cpp/include/nros/client.hpp          :21-29
nros-cpp/include/nros/node.hpp            :37-89
nros-cpp/include/nros/sched_context.hpp   :25-51
nros-cpp/include/nros/stream.hpp          :18-39
nros-cpp/include/nros/transport.hpp       :18-39
nros-cpp/include/nros/future.hpp          :19+
nros-cpp/include/nros/parameter.hpp       :30+
nros-cpp/include/nros/qos.hpp             :20-50
```

**Stated reason** (qos.hpp): "user-facing hpp files can access every field
without pulling the cbindgen-generated FFI header." **Cost:** every cbindgen
update demands manual sync of 16 files, and Phase 112's qos.hpp / node.hpp
divergence already broke the build (4-field shape vs 9-field shape).

- **Fix:** include `<nros_cpp_ffi.h>` from each .hpp that uses FFI types.
  Avoid the "compile-firewall" optimization until profile data shows it
  matters; re-evaluate via `iwyu` / preprocessor-output size if motivated.

#### D.2 (Medium) Naming inconsistency: `_ffi` suffix
- `nros-cpp/include/nros/sched_context.hpp:28-41` declares
  `nros_cpp_sched_context_ffi`.
- `nros_cpp_ffi.h:145-157` declares `nros_cpp_sched_context_t`.
- **Fix:** drop `_ffi` suffix; use cbindgen name.

#### D.3 (Low) Status-callback structs duplicated
- `publisher.hpp:29-34` — `nros_cpp_pub_count_status_t` +
  `nros_cpp_publisher_count_cb_t` typedef.
- `subscription.hpp` — `nros_cpp_liveliness_changed_status_t`.
- Both already in `nros_cpp_ffi.h:205-222`.

C side (`nros-c/include/nros/*.h`) is **clean** — thin shim headers, single
source of truth via `nros_generated.h`.

### 118.E — Build-script self-containment

CLAUDE.md mandates: build.rs must not walk up `CARGO_MANIFEST_DIR` beyond one
level. 8 violations:

#### E.1 (High) Multi-level parent walk-up
- `packages/boards/nros-board-mps2-an385-freertos/build.rs:53,150,174` —
  `manifest_dir.join("../../../third-party/tracing/Tonbandgeraet/tband")` (3
  levels).
- `examples/qemu-arm-nuttx/cmake/nros-nuttx-ffi/build.rs:9` —
  `nros_root = manifest_dir.join("../../../..")` (4 levels).
- `packages/xrce/nros-rmw-xrce-cffi/build.rs:21-30` — chained `.parent()` ×3 +
  no validation.
- `packages/zpico/zpico-platform-shim/build.rs:16` —
  `manifest_dir.parent().unwrap().join("zpico-sys")`; no validation that
  sibling exists.
- `packages/drivers/zephyr-posix-sys/build.rs:24-30` — `.parent()` ×3 + probes
  for external workspace siblings (`../nano-ros-workspace/...`).
- `packages/boards/nros-board-threadx-linux/build.rs:22-28` — workspace_root
  derived from `.parent()` ×3 + uses for `THREADX_DIR` default.
- `packages/boards/nros-board-threadx-qemu-riscv64/build.rs:21-29` — same
  pattern + extra siblings.
- `packages/xrce/xrce-sys/build.rs:17,52-55` — assumes vendored submodules at
  fixed in-crate paths; no env-var override.

**Fix sketch:**
1. Each build.rs accepts an env-var override (`THREADX_DIR`, `NETX_DIR`,
   `MICRO_CDR_DIR`, `TBAND_DIR`, `NROS_ROOT`, etc.).
2. Fallback to walk-up only after env var unset; validate fallback path
   exists (panic with actionable message otherwise).
3. Optional `nros-build-common` crate centralizes the
   `env_path_or(name, fallback)` pattern and project-root probe.

### 118.F — Other antipatterns

#### F.1 (Medium) Dead-code + phase-tag drift
- `nros-node/src/executor/dispatcher.rs:18` — `Dispatcher` trait
  `#[allow(dead_code)]` "Phase 110.A — wired in 110.A.b spin_once rewire".
- `nros-node/src/executor/activator.rs:19,24` — same for `ActivatorCtx`,
  `Activator`.
- `nros-node/src/executor/types.rs:386-410` — `SortKey`, `ActiveJob` dead with
  Phase 110.A tag.
- `nros-node/src/executor/sched_context.rs:27-282` — `OptUs`, `SchedClass`,
  `Priority` etc. dead with multiple phase tags (110.B.a, 110.C, 110.E.b).
- **Fix:** complete Phase 110 wiring or excise the unused scaffolding.

#### F.2 (Medium) Re-export sprawl in nros-c
- `nros-c/src/lib.rs:181-187` — 7 wildcard `pub use module::*;` (cdr, clock,
  constants, error, parameter, qos, transport).
- `nros-c/src/lib.rs:229` — macro-generated wildcard re-exports for action,
  event, executor, lifecycle, node, publisher, service, subscription, support,
  timer.
- `nros-node/src/executor/mod.rs:73,78` — `pub use handles::*;`,
  `pub use types::*;`.
- **Fix:** explicit `pub use module::{Item1, Item2}` lists; document stable
  surface.

#### F.3 (Medium) Feature-flag soup with materially different bodies
- `nros-node/src/executor/handles.rs:37-100` — `WaitBudget` has 3 disjoint
  branches: `std` → `Instant::now()`, `!std + rmw-zenoh` → `z_clock_now()`,
  `!std + !rmw-zenoh` → iteration counter. Each branch has different
  semantics; no shared trait abstracts them.
- 122+ `#[cfg(feature = …)]` directives across nros-node/src/.
- **Fix:** introduce `Clock` trait, three impls (StdClock, ZenohClock,
  IterCounterClock); WaitBudget generic over `C: Clock`.

#### F.4 (Low) Implicit panics in non-test code paths
- `nros-c/src/cdr.rs:565` — `CStr::from_ptr(...).to_str().unwrap()` after
  unsafe cast. Combine `unsafe` + `.unwrap()` is a code-smell.
- `nros-node/src/executor/handles.rs:932,1474` — `.expect("PublishLoan slot
  already consumed")`, `.expect("RecvView accessed after drop")` — defensive
  assertions; panic on API misuse rather than returning error.
- `nros-node/src/executor/spin.rs:3027` —
  `.expect("os-priority worker spawn")` — could fail on systems without thread
  support.
- `nros-rmw-cffi/src/lib.rs:1663` — `.expect("session open")`.
- **Fix (case-by-case):** convert to `Result<_, NodeError>` where the caller
  has a meaningful response; keep `expect` only when invariant truly cannot
  be violated by sound API use.

#### F.5 (Low) Unsafe blocks without `// SAFETY:` comments
- `nros-cpp/src/lib.rs:45,49,74,78,107,111,131,135,520` — FFI-call unsafe
  blocks (FreeRTOS / Zephyr / ThreadX allocators, IRQ control,
  `core::ptr::write` placement-new).
- `zpico-alloc/src/lib.rs:130-137,232-280` — heap init + 48-line free-list
  allocation loop without block-level SAFETY explaining alignment / pointer
  invariants.
- `zpico-platform-shim/src/shim.rs:69,77,85` — three `unsafe { *time }`
  dereferences.
- **Fix:** add `// SAFETY:` block above each, naming the invariant kept by
  the caller.

#### F.6 (Low) Inconsistent error handling across layers
- `nros-c/src/cdr.rs` returns raw `i32` (0 = OK, -1 = fail).
- `nros-node` returns `Result<T, NodeError>`.
- `nros-rmw` per-backend has its own `Result<T, ZpicoError>` /
  `Result<T, XrceError>` etc.
- **Fix:** document the layer-mapping in `docs/reference/error-codes.md`
  (already partially exists?); ensure each conversion is explicit, not
  ad-hoc.

#### F.7 (Low) Dead-code fields not feature-gated
- `nros-node/src/node.rs:68,84,122` — `PublisherInfo`, `SubscriberInfo`, `Node`
  carry fields tagged "used when transport is connected" via
  `#[allow(dead_code)]` instead of `#[cfg(feature = "...")]`. Always-present-
  but-sometimes-unused.
- **Fix:** if fields are truly conditional, gate on feature; else remove the
  attribute and use `let _ = field` at the consumer.

## Acceptance

- [x] **118.audit.scan** — survey complete; findings cataloged.
- [ ] **118.audit.consume** — each remediation rolled into a follow-up
  phase doc (118.A → Phase 119 candidate, 118.B → Phase 87.6 closure,
  118.C → Phase 120 candidate, 118.D → Phase 121 candidate, 118.E → Phase 122
  candidate, 118.F → Phase 123 candidate).

## Notes

- Audit deliberately skipped third-party (`third-party/`, `external/`),
  generated (`build/`, `target/`, `*/generated/`), and the codegen submodule
  (`packages/codegen/`).
- Hand-math (118.B) is the only category currently considered "controlled" —
  the compile-time assert chain prevents silent corruption. Other categories
  have no automated guard.
- Magic-number cleanup (118.A) is the lowest-risk batch — most fixes are
  rename + extract-const without behavior change.
- Wrapper-violation cleanup (118.C) and bindgen mirrors (118.D) are the
  largest user-visible improvements — they pay forward into faster Phase 110
  / future async work.
