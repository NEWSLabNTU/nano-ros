# Phase 91: Code Antipattern Fixup

**Goal**: Resolve the antipatterns surfaced by the April 2026 cross-cutting
code audit. Six antipattern categories were swept (hardcoded paths /
references to the source tree, magic numbers, manual size math, non-thin
C/C++ wrappers, duplicated functions, C/C++ mirrors not backed by
cbindgen). The audit found two categories essentially clean (magic
numbers, manual size math) and four with concrete debt that should land
as small, independently-mergeable PRs.

**Status**: In Progress (Groups A, B, C, D, E3, E4 complete; E1/E2/F/G remaining)
**Priority**: Medium — none of these block users today, but several are
direct repeat findings against phases that were marked Complete (Phase 83
"thin-wrapper compliance"; Phase 87 "cbindgen-driven headers" per the
CLAUDE.md narrative). Letting the gap widen invites the same audit to
re-fire in three months.
**Depends on**: None. Group B (thin-wrapper follow-ups) overlaps in spirit
with Phase 84 Group B; if the Phase 84 backlog reopens, fold these items
in there instead of duplicating.

## Overview

The audit ran six parallel sweeps with the explicit framing "find concrete
antipatterns, don't speculate." Findings:

| Category | Status |
|---|---|
| Hardcoded paths / source-tree refs | Debt — 4 `/tmp/` sites in test code, 11+ port-7447 literals in zenoh integration tests |
| Magic numbers | Clean — buffer sizes, port table, CDR constants, stack sizes all named |
| Manual size math | Mostly clean — two documented fallbacks (Phase 77.23 LTO probe failure + Phase 87 transitional executor upper bound), both compile-time-asserted |
| Non-thin C/C++ wrappers | Debt — `nros-c` directly imports `nros_rmw` / `nros_core` from 7 sites; `~14` repeated string-copy loops; hand-defined entity structs |
| Duplicated / similar functions | Debt — platform `seed()` copy across freertos+threadx; ThreadX cmake support files; C++ example startup boilerplate; service/client setup parallels in `nros-node` |
| C/C++ mirrors not backed by cbindgen | Debt — `nros_generated.h` is generated for "drift detection" only; real per-module headers (`executor.h`, `publisher.h`, `service.h`, `node.h`, `subscription.h`, `timer.h`, `client.h`, `lifecycle.h`, `init.h`, `guard_condition.h`) hand-define every entity struct |

Two surprising results worth recording:

1. **`nros_generated.h` is dead code from the consumer's perspective.** It is
   produced by `cbindgen` during `cargo build` (per `nros-c/build.rs:225`
   and `cbindgen.toml`), `.gitignore`d, and not `#include`d anywhere — the
   `Doxyfile` (`Doxyfile:10`) explicitly excludes it as an "internal cbindgen
   artifact for drift detection." This is the opposite of the model
   described in `CLAUDE.md` ("`nros_generated.h` is included by thin
   per-module header stubs"). Either CLAUDE.md should reflect reality
   (cbindgen-as-a-linter) or the per-module headers should actually source
   their struct definitions from the generated file. This phase picks the
   latter.
2. **Phase 83's "thin wrapper" claim is partially false.** The audit found
   `use nros_rmw::*` / `use nros_core::*` imports inside `nros-c` at 11
   distinct call sites (verified: `cdr.rs:9`, `qos.rs:90`, `lifecycle.rs:11`,
   `publisher.rs:234,333`, `support.rs:162`, `service.rs:740,840,885,952`,
   `action/server.rs:647`). These should funnel through `nros-node` per the
   stated principle. Phase 84.B2 already moved the bulk of CDR to
   `nros-serdes`; the remaining 11 sites are smaller surgical extractions.

## Work Items

### Group A — Hardcoded paths and ports

- [x] 91.A1 — Replaced 3 `PathBuf::from("/tmp/test*")` sites in `cache.rs` with `tempfile::tempdir()` (already a dep). The remaining `PathBuf::from("/nonexistent/path")` in `test_is_valid_output_missing` is intentional — that test exists to prove `is_valid` returns false on a missing dir
- [x] 91.A2 — Added `const ROUTER_LOCATOR: &str = "tcp/127.0.0.1:7447"` and replaced 6 in-body locator literals + 1 `eprintln!` setup hint. Remaining 5 occurrences are inside `#[ignore = "..."]` attributes (Rust attributes don't accept const substitution) and 1 doc comment. All 4 non-router tests pass; the 5 router-required tests stay properly `#[ignore]`d
- [x] 91.A3 — Confirmed: vendored upstream `/tmp/serial_fifo` literals stay as-is. Recorded as upstream debt; no fork
- [x] 91.A4 — Verified: `nuttx-support.cmake:39` uses `NanoRos_DIR` install-prefix from `find_package` — portable, no change needed. `freertos-support.cmake:23–32` does default `FREERTOS_DIR`/`LWIP_DIR` to `${_NROS_ROOT}/third-party/...` (env-var override always wins, so in-tree builds work). Technically violates the relocatable-example rule but only triggers when env var is unset. Left as-is for this phase; flagged for cmake-helper consolidation in Group E1

### Group B — `nros-c` thin-wrapper follow-ups (extends Phase 83 / 84.B)

These are the 11 audit-confirmed sites where `nros-c` imports `nros_rmw` /
`nros_core` directly instead of going through `nros-node`. Each item lands
its own PR: add a small surface to `nros-node`, then collapse the
`nros-c`-side import.

Implemented as a sequence: one *prep* commit adding all needed re-exports
to `nros-node` (route is established once, then each per-file migration
becomes a clean rename PR).

- [x] 91.B-prep — Added `pub use nros_rmw::{Session, Publisher, Subscriber, ServiceClientTrait, ServiceServerTrait}`, `pub use nros_core::{CancelResponse, GoalId, GoalResponse, GoalStatus}`, `pub use nros_core::lifecycle::{LifecycleState, LifecycleTransition, TransitionResult}`, and (closer) `pub use nros_core::{CdrReader, CdrWriter, DeserError, SerError}` to `nros-node`. Pure additive; no caller renamed yet
- [x] 91.B1 — `nros-c/src/qos.rs` migrated: `nros_rmw::{QosSettings, QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy}` → `nros_node::*`. Pure path rename
- [x] 91.B2 — `nros-c/src/lifecycle.rs:11` migrated: `nros_core::lifecycle::*` → `nros_node::*`. Pure path rename
- [x] 91.B3 — `nros-c/src/publisher.rs:{234,333}` (`Session`, `TopicInfo`, `Publisher`) and `support.rs:162` (`SessionMode`) migrated to `nros_node::*`. Pure path rename
- [x] 91.B4 — `nros-c/src/service.rs:{740,840,885,952}` (`ServiceClientTrait`) migrated to `nros_node::*`. Pure path rename
- [x] 91.B5 — Migrated *all* (~30 sites, not just the audit-flagged test-only one): `nros_core::{GoalId, GoalStatus, GoalResponse, CancelResponse}` → `nros_node::*` across `executor.rs`, `action/server.rs`, `action/client.rs`. The audit's narrow read of B5 (one test-only `use`) was the tip of a much wider iceberg
- [x] 91.B6 — Collapsed 15 hand-rolled `while len < MAX_*_LEN - 1 { ... }` C-string-copy loops across `publisher.rs`, `subscription.rs`, `node.rs`, `support.rs`, `service.rs`, `action/server.rs`, `action/client.rs` into a single `pub(crate) unsafe fn copy_cstr_into<const N: usize>(src, dst) -> usize` helper in new `nros-c/src/util.rs`. Required-string sites still check `if len == 0 { return INVALID_ARGUMENT; }`; optional-string sites just call it. Net: ~270 lines removed, ~50 added; the 5 init paths taking a TopicInfo-style triple now read in 6 lines instead of ~50

After this group: `git grep -nE 'use (nros_rmw|nros_core)::' packages/core/nros-c/src/` returns **zero** hits — Group B's acceptance criterion is fully met.

### Group C — Wire `nros_generated.h` into the public include path

Cbindgen output is now the single source of truth for nros-c's C API
surface. Each public struct, enum, callback typedef, and function
declaration is generated from the Rust `#[repr(C)]` / `unsafe extern "C"`
definition and mirrored exactly. Drift between Rust and C is caught at
cbindgen-run time (every cargo build).

The path here zigzagged: an earlier attempt dropped cbindgen entirely
(option B) on the (correct) observation that the existing output
referenced undefined macros. On the (correct) pushback that cbindgen's
field-exact mirroring is a stronger SSoT guarantee than what
size-only asserts could replace, the option-B commits were dropped
unpushed and option A was redone properly.

- [x] 91.C1 — Chose option A (consume cbindgen output). Three blockers
  fixed in `nros-c/build.rs` + `cbindgen.toml`:
  1. cbindgen-emitted `*_OPAQUE_U64S` placeholders had wrong values
     (cbindgen ran without RMW features so `u64s_for::<T>()` returned
     placeholder 1). Suppressed via `[export.exclude]`; build.rs now
     emits the real values into `nros_config_generated.h`.
  2. `EXECUTOR_OPAQUE_U64S` / `GUARD_HANDLE_OPAQUE_U64S` /
     `NROS_LIFECYCLE_CTX_OPAQUE_U64S` were referenced but not defined
     anywhere C-visible. Same fix as (1).
  3. cbindgen-emitted `ActionServerInternal` references
     `nros_node::ActionServerRawHandle` as a typed inline field, but
     `parse_deps = false` means cbindgen can't see its body. Added
     `ACTION_SERVER_RAW_HANDLE_SIZE` to `nros::sizes` (probed) and
     `build.rs` emits a typedef-compatible opaque definition into
     `nros_config_generated.h` so the cbindgen output is fully
     self-contained when included from C / C++.
  Plus: forward declarations of `nros_*_t` struct tags injected into
  the cbindgen header preamble so callback typedefs that reference them
  through parameter lists don't trip `-Werror=incompatible-pointer-types`
- [x] 91.C2 — `executor.h` migrated. Now a thin shim (file-header +
  `#include "nros/types.h"`); the typed `nros_executor_t` body comes
  from cbindgen
- [x] 91.C3 — Same migration for `init.h`, `publisher.h`,
  `subscription.h`, `service.h`, `client.h`, `node.h`, `timer.h`,
  `guard_condition.h`, `lifecycle.h`, `action.h`, plus `clock.h`
  (which had the same duplication). `parameter.h` is a hybrid: types
  come from cbindgen but its `paste!`-generated FFI function
  declarations stay hand-written because cbindgen's `[parse.expand]`
  needs nightly Rust to see them. Re-enable when the project moves
  to nightly
- [x] 91.C4 — `CLAUDE.md` "cbindgen header generation" paragraph
  retained — it now matches reality
- [N/A] 91.C5 — CI grep for redefined structs is moot because per-module
  headers no longer have any struct bodies to grep for. If they later
  drift, the `#include` brings cbindgen's definition in and any duplicate
  in the per-module header would be a hard compile error, caught
  immediately

After this group: per-module headers shrink by ~3127 lines total
(typical file: 95% reduction), `types.h` becomes a 26-line shim, and
the only per-module header still carrying hand-written FFI function
declarations is `parameter.h` (with a comment explaining the `paste!`
macro reason).

### Group D — Platform crate deduplication

- [x] 91.D1 — Added `pub mod xorshift32` to `nros-platform-api` exposing `step`, `next`, `seed`, `random_fill`, and `DEFAULT_SEED`. Migrated freertos and threadx platform crates to call into the helpers; each crate keeps its own `static mut RNG_STATE` (callers own the state cell). Net: ~30 lines of duplicated logic removed per platform. 7 new unit tests in the api crate cover step determinism, seed-zero fallback, null-buf no-op, and length correctness — all pass. C-callable seeders (`nros_platform_freertos_seed_rng`, `nros_platform_threadx_seed_rng`) keep their existing symbol names so `startup.c` / `app_define.c` don't need rebuilds
- [x] 91.D2 — Surveyed the remaining four `PlatformRandom` impls: `posix` uses libc `fill_random` (no dup), `nuttx` delegates to `PosixPlatform` (no dup), `zephyr` calls `sys_rand32_get` (no dup), `cffi` forwards through a vtable (no dup). `yield_now` impls are all platform-specific syscall wrappers (`sched_yield`, `tx_thread_relinquish`, `k_yield`, `taskYIELD` shim, vtable forward) — no duplication. The audit's hypothesis was right: only freertos+threadx had the xorshift copy

### Group E — Example boilerplate consolidation

**Group E cmake design — three-layer abstraction (applies to E1).**

A naive 2-file dedup of the two ThreadX support files would buy ~80
lines but leave the next platform port reinventing the boilerplate
from scratch. Designing for N future platforms instead, the structure
is:

| Layer | What it does | Where it lives |
|---|---|---|
| 1. Cross-RTOS primitives (`nros-rtos-helpers.cmake`) | `nros_validate_vars`, `nros_build_rtos_static_lib`, `nros_compose_platform_target` — pure mechanics, knows no specific RTOS | shipped via `find_package(NanoRos)` install (Phase 75) |
| 2. Per-RTOS module (`nros-threadx.cmake`, `nros-freertos.cmake`, …) | `nros_<rtos>_build_kernel`, `nros_<rtos>_build_netstack_*`, `nros_<rtos>_compose_platform`, plus optional `nros_<rtos>_setup_picolibc` / `setup_rust_lld` / `strip_builtins` for ports that need them. Encodes RTOS-specific quirks (port subdirs, source globs, kernel-flavored asserts) once. | shipped via the same install |
| 3. Per-platform orchestrator (`<plat>-support.cmake`) | 10–20 lines: set platform-specific knobs (port subdir, libs, link script), call layer-2 functions | stays in `examples/<plat>/cmake/` (CLAUDE.md "examples must remain portable") |

A new platform port becomes "copy a 15-line layer-3 file, change the
port subdir and link libs". A new RTOS becomes "write `nros-<rtos>.cmake`
using the layer-1 primitives" (~80 % mechanical).

**Naming convention.** Layer-1 functions: `nros_<verb>_<noun>`. Layer-2:
`nros_<rtos>_<verb>_<noun>`. Long but unambiguous; clear namespace
separation keeps cmake's flat function namespace from collapsing into
a soup of `build_kernel` collisions when more RTOSes land.

**Variable convention.** Long-term we want `NROS_<RTOS>_<COMPONENT>_DIR`
(e.g. `NROS_THREADX_KERNEL_DIR`) instead of the per-file `THREADX_DIR`,
`NETX_DIR`, `FREERTOS_DIR`, `LWIP_DIR`, `NUTTX_DIR` zoo. **Not done in
E1** — that's a backward-compat-breaking rename and warrants its own
deprecation pass. New layer-2 functions should accept both forms during
a transition.

- [ ] 91.E1a — Layer 1 (`nros-rtos-helpers.cmake`) + layer 2 ThreadX (`nros-threadx.cmake`) shipped via the cmake install. Rewrite `examples/threadx-linux/cmake/threadx-support.cmake` (88 lines) and `examples/qemu-riscv64-threadx/cmake/threadx-riscv64-support.cmake` (213 lines) as thin orchestrators on top of them. Validates the design against the file with the most variation (RISC-V's assembly excludes / picolibc / rust-lld plumbing). **Acceptance**: `just threadx_linux build-fixtures` and `just threadx_riscv64 build-fixtures` succeed before/after the refactor with equivalent artefacts (build-fixtures recipes from upstream commit `0e5e03a1` make this easy to verify).
- [ ] 91.E1b — Layer 2 FreeRTOS (`nros-freertos.cmake`) + rewrite `examples/qemu-arm-freertos/cmake/freertos-support.cmake`. Separate PR; if E1a's design needs adjustment after touching another RTOS, this is where it surfaces.
- [ ] 91.E1c — Layer 2 NuttX (`nros-nuttx.cmake`) + rewrite `examples/qemu-arm-nuttx/cmake/nuttx-support.cmake`. NuttX is awkward because it leans on kconfig more than cmake; `nros-nuttx.cmake` may end up thinner than the others or just expose a different shape (e.g. invoke NuttX's own build).
- [ ] 91.E2 — Same for the C++ example startup boilerplate across `examples/threadx-linux/cpp/zenoh/talker/`, `examples/qemu-arm-nuttx/cpp/zenoh/talker/`, `examples/qemu-riscv64-threadx/cpp/zenoh/talker/`. Candidate: ship a `nros::examples::pubsub_helpers` header (header-only) in `nros-cpp/include/nros/examples/`, gated by an opt-in macro so production users don't link it
- [x] 91.E3 — Created `tests/lib/common.sh` exposing colors (RED/GREEN/YELLOW/BLUE/CYAN/NC), 5 log functions (`log_info`/`log_success`/`log_warn`/`log_error`/`log_header`), `register_pid`/`cleanup_pids`, and `init_test_tmpdir`/`cleanup_test_tmpdir`/`tmpfile`. `tests/c-msg-gen-tests.sh` was rewritten to source it (renamed `info`/`warn`/`error` → `log_info`/`log_warn`/`log_error` at 16 call sites). `tests/zephyr/run-c.sh` was rewritten to source it (dropped 60+ lines of inlined helpers; cleanup function now delegates to `cleanup_pids`/`cleanup_test_tmpdir`). Both scripts pass `bash -n`; helper smoke-tested independently
- [x] 91.E4 — Added `_nextest-platform <test_name> [verbose]` private recipe to root justfile. Refactored `just/freertos.just`, `just/nuttx.just`, `just/threadx-linux.just`, `just/threadx-riscv64.just` `test` recipes to call it via `just _nextest-platform <name> '{{verbose}}'` after their pre-flight checks. Cross-module dispatch verified by running `just nuttx test` end-to-end (3/3 nuttx_qemu tests pass). The `test-all` recipes have additional pre-build / network-setup steps and weren't collapsed in this pass — leaving them for a future cleanup if the duplication grows

### Group F — `nros-node` service/client setup symmetry

- [ ] 91.F1 — `node.rs:136–162` (`create_service_*`) and `node.rs:165–192` (`create_client_*`) are structurally parallel: ServiceInfo construction, session method call, buffer allocation, error mapping. Extract the shared body into a `fn build_service_handle<S, F>(info, allocator, finalize: F)` helper. Bar for landing: zero behaviour change, both functions reduce to ~10 lines each

### Group G — Documentation truth-up

- [ ] 91.G1 — Update `CLAUDE.md` Phase table: Phase 83 is currently marked Complete but the audit found 11 live `nros_rmw` / `nros_core` imports. Either re-open Phase 83 (preferred), or add a "follow-up" footnote pointing at this phase
- [ ] 91.G2 — Update `CLAUDE.md` "C API" paragraph if 91.C1 chooses to drop cbindgen — the "auto-generated from Rust `#[repr(C)]` types … `nros_generated.h` is included by thin per-module header stubs" sentence is currently aspirational, not descriptive

## Acceptance Criteria

- [ ] Group A: zero `/tmp/test*` literals in `packages/codegen/`; zero hardcoded `7447` literals outside the platform port table or doc-comment examples
- [ ] Group B: `git grep -nE 'use (nros_rmw|nros_core)::' packages/core/nros-c/src/` returns either 0 hits or only the items explicitly recorded as "kept by design" in this doc
- [ ] Group B6: `git grep -c 'while len < MAX_' packages/core/nros-c/src/` ≤ 1 (the helper itself)
- [ ] Group C: per the 91.C1 decision — either no `nros_generated.h` is produced, or every per-module header `#include`s it and contains zero `typedef struct nros_xxx { ...fields... }` bodies
- [ ] Group D: `pub fn seed(value: u32)` defined in exactly one platform crate
- [ ] Group E: `tests/lib/common.sh` sourced by both `run-c.sh` and `c-msg-gen-tests.sh`; the duplicated cmake helper module is referenced by both ThreadX example trees
- [ ] All of `just ci`, `just test-all`, and `just verify` pass (no test deletions to make this true — fix the underlying issue or `#[ignore]` with a tracked ticket)

## Notes

- **What the audit explicitly did not find.** Magic-number antipatterns and
  manual-size-math antipatterns are recorded as clean (or with documented,
  compile-time-asserted exceptions). No work items in this phase target
  them. If a future audit re-fires those categories, look at
  `nros-c/src/opaque_sizes.rs`, `nros-cpp/build.rs`, and
  `nros-c/build.rs:76–86` first — those are the "watched" sites.
- **Vendored third-party code is out of scope.** The `/tmp/serial_fifo`
  literals in `packages/xrce/xrce-sys/micro-xrce-dds-client/` are upstream;
  do not patch them in-tree. If serial-FIFO test isolation becomes a real
  problem on shared CI, fix it via process-scoped temp paths, not by
  forking the upstream tree.
- **Group C is the highest-stakes item.** Touching every public
  per-module header in `nros-c` is an ABI-adjacent change. Run the full
  C/C++ example matrix (`just native test-all`, `just freertos test-all`,
  `just nuttx test-all`, `just threadx_linux test-all`,
  `just threadx_riscv64 test-all`, `just zephyr test-all`) before merging
  C2. The `lifecycle.h` migration in particular interacts with Phase 84.B4
  / Phase 86.4.
- **Order of operations.** A → D → E → F → B → G → C. Group C is last
  because the cbindgen consumption decision (91.C1) wants to land after the
  thin-wrapper imports are minimised — otherwise the generated header has
  to mirror types that are about to move.
