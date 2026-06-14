# Phase 241 — Platform & build determinism

**Implements.** [RFC-0042](../design/0042-platform-build-determinism.md) — one
canonical platform interface, capability-driven config, deterministic linking,
merge-time gate.

**Goal.** End the recurring class of C/C++/Rust build failures (libc/std header
clashes #27/#36/#38, ld single-pass undefined-symbol races #20, silent capability
mismatches) by making the build contract *structural* instead of
convention-enforced. Cross-refs the systemic review in [issue 0042].

**Status.** In progress (2026-06-12 → ongoing). **241.A (gate)** + **241.C
(capability SSoT)** LANDED; **241.B (one canonical header)** LANDED — B.3 (the ABI
unification) was carved to [phase-243](phase-243-platform-abi-unification.md),
which LANDED on main (the legacy `nros-c` `<nros/platform.h>` is deleted). **241.D
(deterministic linking, RFC-0042 D3)** is the remaining open pillar: its current
design is the single shared runtime
([phase-241-d3-single-runtime](phase-241-d3-single-runtime.md)) + the one
registration trigger ([phase-249](phase-249-one-registration-trigger.md) / issue
#62) — the original slice-4 `nros-rmw-cffi-provider`/`external-registry` dedup is
**retired** (verified: no provider crate, no `external-registry` feature on main).
241.E (RFC flips + #42 close) follows D.

**Priority.** P2 — no product capability is blocked, but this class of bug recurs
on nearly every board/example/platform-header edit, and each instance currently
surfaces only in an on-demand e2e build (days late). Continuation of the
[Phase 240](archived/phase-240-ci-disk-and-build-optimization.md) honest-e2e work
that exposed the pattern.

**Depends on.** RFC-0042; the Phase 195.C board-descriptor mechanism (capability
block extends it); the per-platform CI matrix (Phase 196 / 240) the gate plugs
into.

---

## Waves

Order: **A (gate) → C (capability SSoT) → B (header collapse) → D (linking) → E.**
The gate lands first (safety net). C precedes B because the header collapse needs
a settled capability source — the capability macros are produced by A's per-RTOS
sub-header dispatch today, so the canonical header can't be authored until C
decides where capabilities come from (see the B↔C coupling note under 241.B).
Each wave is independently revertible.

### 241.A — Merge-time compile gate (RFC-0042 D4) — FIRST
Two tiers, by what a cell needs to compile:

- [x] **Host tier (landed; all platforms — extended 2026-06-13).**
      `packages/testing/nros-tests/tests/platform_header_matrix.rs`
      + `.github/workflows/platform-header-gate.yml`: drives host `g++`/`cc` over
      the real `<nros/platform.h>` + the nros-cpp heap containers, asserting
      positive AND negative outcomes — bare-metal **without**
      `NROS_PLATFORM_HAS_MALLOC` MUST fail to compile the heap containers, **with**
      it MUST succeed (the #38 mechanism, both directions). **Originally scoped to
      POSIX + bare-metal** (the per-platform nros-c sub-headers pulled the RTOS
      sysroot). After the D1 collapse (241.B / 243.B.5) `<nros/platform.h>` is the
      ONE self-contained `nros-platform-api` header — no RTOS include — so the
      heap-container compile is host-cheap for **every** platform. Extended to add
      one heap cell per RTOS target (FreeRTOS/Zephyr/ThreadX/NuttX/ESP), closing
      the #42 root-cause-#5 gap ("FreeRTOS/Zephyr/ESP+C++ had no isolated compile
      test"). Cheap (no SDK, no cross target, ~seconds); mirrors the `core-libs`
      lane; PR-gated on the platform headers + `nros-board.toml` + the test.
- [ ] **Cross tier (later) — narrowed.** Only the two-libc-set class (#27/#36)
      remains cross-only: it bites the platform **`.c` TUs** (`#include_next`
      reaching the wrong libc), not the self-contained header above, so it still
      needs the cross toolchain + RTOS sysroot. Options: (a) provision the cross
      toolchain + a minimal vendored RTOS header stub to reproduce the clash, or
      (b) compile the RTOS cpp examples on the PR path (heavier; needs the export).
      Until then the `.c`-TU class stays covered by the e2e `build-fixtures` lane
      (Phase 240).
- **Acceptance (host tier):** ✅ a PR that reintroduces the #38-class capability
      breakage goes red on the gate, not green-then-red-in-e2e. Verified locally:
      the 10-cell matrix passes (POSIX + bare-metal + one heap cell per RTOS
      target), the bare-metal-no-malloc negative cell fails to compile as required.

### 241.B — Collapse to one canonical interface (RFC-0042 D1)

Direction (corrected, see RFC-0042 D1): **B (the cffi full ABI) is canonical, A
(nros-c legacy) is retired, `nros-platform-api` owns the one header.** The two
files share name + guard `NROS_PLATFORM_H` today → include-order ABI poison.
Staged so each step is CI-validated before the next; the 241.A gate + the existing
`c_stub_platform.rs` parity test guard the churn.

> **Execution note (2026-06-12, after reading the full surface).** B is **one
> high-blast-radius change** — it deletes a header (`nros-platform-cffi`'s 349-line
> canonical ABI) that ~20 include sites + every platform port + xrce/cyclone/
> zpico/smokes resolve, and moves the hand-written Rust mirror + the
> `c_stub_platform.rs` parity test. It **cannot be locally cross-validated** (RTOS
> builds need their exports) and cannot be sliced without dead-weight (a second
> file named `nros/platform.h` re-creates the include-order race). Execute it as a
> **dedicated, CI-monitored run** (host gate + a `run_e2e` dispatch after the
> rewire), not a session-tail blind push. The design + exact targets below are
> settled so the run is mechanical.

Target design (canonical header, self-contained in `nros-platform-api` — breaks
the nros-c↔cffi tangle):
- Body = the current `nros-platform-cffi/include/nros/platform.h` ABI **verbatim**
  (keeps `lib.rs` parity; `c_stub_platform.rs` moves with it).
- The unconditional malloc/free shim becomes **gated**: `#ifdef
  NROS_PLATFORM_HAS_MALLOC` … forward to `alloc`/`dealloc` … `#endif`. Preserves
  the 241.A #38 compile-gate (no-heap board → no malloc/free → heap-container use
  is a *compile* error, not a link error).
- A **small self-contained capability-default block** (`#if defined(NROS_PLATFORM_POSIX)`
  → `#define NROS_PLATFORM_HAS_MALLOC`/`HAS_ATOMICS`; bare-metal → atomics only;
  …) supplies the platform-*constant* defaults — NOT a re-host of A's legacy
  sub-headers (those carry the ns-clock/typed-mutex legacy ABI and are retired).
  The variable case (bare-metal heap) comes from C.2's board.toml `-D`.

Steps (each a commit; CI between the riskier ones):
- [x] **B.1 — canonical header authored (landed, additive).**
      `packages/core/nros-platform-api/include/nros/platform.h` = the cffi ABI
      verbatim + the self-contained capability-default block (posix → HAS_MALLOC +
      HAS_ATOMICS; others → HAS_ATOMICS, heap opt-in via the C.2 `-D`) + the
      malloc/free shim gated on `NROS_PLATFORM_HAS_MALLOC`. Validated with host
      g++ across the 241.A cells: posix → malloc OK; baremetal no-`-D` → malloc
      *and* the nros-cpp heap containers fail to compile (correct #38 gate);
      baremetal +`-D` → OK. Additive (not yet on any `-I`); the Rust mirror +
      `c_stub_platform.rs` stay in cffi until B.2 deletes cffi's copy. **One-step
      duplication window**: api's header diverges from cffi's (gated vs
      unconditional shim) until B.2 rewires + deletes cffi's.
- [x] **B.2 wave 1 — api block forward-ready (landed, additive).** The capability
      block now defines `HAS_MALLOC` for every heap platform so nothing breaks when
      consumers switch to it: POSIX-family (posix/nuttx/threadx-linux/native, all
      POSIX-mapped) + the heap RTOSes (zephyr/freertos). Bare-metal/ThreadX-RV64/
      ESP stay heap-opt-in via the board.toml `-D` (C.2). Validated host g++:
      POSIX/ZEPHYR/FREERTOS heap-container compile OK; bare-metal no-`D` FAIL
      (correct #38 gate); bare-metal +`D` OK. Still additive (api not on any `-I`).
      *ESP gap: resolved/moot* — ESP builds no cpp examples or cpp-heap fixtures
      (`examples/esp32` + fixtures.toml have no esp cpp), so the heap-container
      compile is never exercised on ESP; the forward won't break it.
- [x] **B.2 wave 2 — the forward/rewire (LANDED on main, 2026-06-12).** Deleted
      cffi's `platform.h`; moved its `platform_{net,timer,zephyr}.h` siblings to
      `nros-platform-api/include`. Repointed every consumer to api: the
      `nros-build-paths` helper, the `NROS_PLATFORM_CFFI_INCLUDE` env (`sdk-env.just`),
      ~20 cmake/build.rs/toml sites + the shell/Makefile sites the first e2e caught
      (`scripts/qemu/build-zenoh-pico.sh` was the qemu breaker). **Zero tracked code
      files reference `cffi/include`.** Validated on branch `run_e2e`:
      **5/6 cells green** (qemu, esp32, freertos, threadx_linux, threadx_riscv64 full
      incl. cpp/cyclone/xrce). nuttx red is a **pre-existing main regression** (the
      `240.6` NuttX-talker migration — undefined `nros_platform_*` are Rust
      `#[no_mangle]` symbols, header-independent; origin/main fails identically),
      not this rewire. Local: ABI parity (`c-stub-test`), `posix-c-port`, the 241.A
      gate, and a `cpp_talker` ELF all clean. Merged ff to main (`62ea551eb`).
      <details><summary>original wave-2 plan</summary>
      Repoint the chokepoints
      `nros-build-paths::nros_platform_cffi_include()` + the
      `NROS_PLATFORM_CFFI_INCLUDE` cmake/env var (→ `nros-platform-api/include`),
      plus the literal-path cmake sites (nros-c:137; nros-platform-{posix,freertos,
      zephyr,threadx,nuttx,esp-idf} CMakeLists; xrce:33/35/198; the 3 *-c-smoke
      CMakeLists; zephyr/CMakeLists:51 + nros_cargo_build.cmake:99/287). **Delete**
      `nros-platform-cffi/include/nros/platform.h`. (Prefer the include-dir rewire
      over a relative-`#include` forward — the latter is fragile under the
      build-zenoh copy-out.) → CI `run_e2e`; xrce/cyclone are the cell-reddening
      risks; iterate per red cell.
      - **Mechanics note:** no single chokepoint flips it. `nros_platform_cffi_include()`
        returns one path that consumers ALSO use for cffi's other headers
        (`platform_net.h`, `platform_timer.h`), so each of the ~20 consumers must
        *add* `nros-platform-api/include` AHEAD of `nros-platform-cffi/include`
        (then `<nros/platform.h>` resolves to api's; cffi's siblings still found),
        and cffi's `platform.h` is deleted. Add a sibling
        `nros_platform_api_include()` helper for the build.rs callers. Irreducibly
        ~20 edits; run as a focused pass with a `run_e2e` dispatch, not a
        session-tail blind push.
      </details>
> **B.3 RE-SCOPED (2026-06-12) — A is a live complementary ABI, not legacy.**
> Investigation for B.3 found `nros-c/include/nros/platform.h` (+ its per-RTOS
> sub-headers) is NOT a retirable duplicate. It uniquely provides a surface the
> api/B header lacks, and that surface is **in active use**:
> - **ns-clock** `nros_platform_time_ns` / `sleep_ns` — `nros-board-*/startup.c`,
>   `nros-rmw-cyclonedds/src/internal.hpp`, `nros-cpp/src/lib.rs`,
>   `examples/native/c/custom-platform`.
> - **atomics** `nros_platform_atomic_store_bool` / `load_bool` — per-platform
>   (memory-barrier inline on bare-metal, `__atomic` on POSIX, `atomic_set/get`
>   on Zephyr); used by the board `startup.c` + custom-platform.
> - typed mutex `nros_mutex_t` (FEATURE_THREADS).
>
> api/B is a *different* live surface (ms/us clock, alloc/dealloc, tasks, condvar,
> log). Some consumers (e.g. `nros-cpp`) use symbols from BOTH. So these are two
> live platform ABIs that happen to share the `<nros/platform.h>` name + guard —
> deleting A breaks real builds. A true single canonical header requires
> **unifying the two ABIs**: absorb A's atomics (re-encode the per-platform
> impls), ns-clock, and typed-mutex into the api header (or migrate consumers off
> them to api's clock/mutex), keep `lib.rs` parity, and validate across every
> platform. That is a design step (an RFC-0042 amendment), not a mechanical wave —
> and higher-risk than B.2 since it's an ABI change, not an include-path move.
> **Recommendation:** treat B.3–B.5 as a separate scoped effort; B.2 already
> delivered the concrete win (the cffi duplicate is gone, one fewer `platform.h`).

- [x] **B.3 — unify A's surface into api, then retire A. DONE via
      [phase-243](phase-243-platform-abi-unification.md) (LANDED on main).** Grew
      from "delete the duplicate" into a real ABI unification (A was a *live*
      complementary surface) → carved to phase-243 (6 waves). api gained generic
      `__atomic` atomics; the Rust `platform.rs` wrappers re-point to
      `core::sync::atomic` + `clock_us`; `time_ns`/`sleep_ns` + the dead typed-mutex
      dropped; POSIX heap funnels through `nros_platform_alloc`; the legacy `nros-c`
      `<nros/platform.h>` + its 4 sub-headers are deleted (`git ls-files` = 0).
- [x] **B.4 — parity assert. DONE (243.6).** `c_stub_platform.rs` compiles clean
      on main → the canonical header ↔ `nros-platform-cffi/src/lib.rs` parity guard
      is intact post-collapse (atomics inline → not mirrored; rest unchanged).
- [x] **B.5 — repoint the 241.A gate at the canonical header (landed).**
      `platform_header_matrix.rs` lists `nros-platform-api/include` as the first
      `-I` (243.B.5), so `<nros/platform.h>` resolves to the canonical header; the
      `core-no-malloc` atomics cell passes against api and the negative #38 cell is
      kept. The 2026-06-13 all-platforms extension drives one heap cell per RTOS
      target off the same canonical header.
- **Acceptance:** exactly one file named `nros/platform.h`; `#include
      <nros/platform.h>` resolves identically regardless of `-I`/`-isystem` order;
      all per-platform CI cells (incl. xrce/cyclone) green via `run_e2e`;
      #36/#38 reproductions stay fixed.

> **Note — include precedence (#27/#36, the two-libc class) is NOT in B.** That is
> the cross-toolchain/RTOS-sysroot concern; it lands in 241.D's shared
> precedence helper (and the 241.A cross tier). B is purely the
> one-canonical-header collapse.

> **Coupling found in B.1 (2026-06-12): B depends on C's capability home.** The
> capability macros are NOT centralized in A's `platform.h` — they are *produced*
> by A's compile-time dispatch to the per-RTOS sub-headers (`posix.h` `#define`s
> `NROS_PLATFORM_HAS_MALLOC`/`HAS_ATOMICS`/`HAS_MUTEX`; `baremetal.h` `#define`s
> `NROS_NO_DYNAMIC_MEMORY`; etc.). So "fold the capability macros into the one
> canonical header" can't be answered without deciding where capabilities *come
> from*. Two orders:
> - **(rec) Do 241.C's capability-macro home first** (board.toml `[board.capabilities]`
>   → generated `-D`s), then the canonical header simply *consumes* the generated
>   `NROS_PLATFORM_HAS_*` and the per-RTOS sub-headers + their `#define`s are
>   retired. Clean: the collapse lands on a settled capability source.
> - **(interim) Canonical header keeps A's dispatch-to-sub-headers** purely for the
>   capability `#define`s while B's ABI is the body — a transitional two-mechanism
>   header, removed when C lands. Faster to the single file, but carries the
>   dispatch lore B was meant to kill.
> Recommendation: reorder to **C before B** (or do C's macro-home slice first),
> since B's value (one clean header) is undercut if it must re-host A's dispatch.

### 241.C — Capability-driven config SSoT (RFC-0042 D2)
- [x] **C.1 — schema + parse + populate (landed).** `[board.capabilities]`
      (`heap`/`atomics`/`threads`) added to `BoardDescriptor` (mirrors the
      `[board.entry]` nested-table pattern) with a `capabilities()` resolver that
      falls back to platform-inferred defaults + a `has_declared_capabilities()`
      helper for the migration lint. All 12 board.toml (13 entries) populated:
      heap-capable = posix/zephyr/freertos/nuttx×2/threadx×2/esp32×2; heap-less =
      baremetal/orin-spe/stm32f4×2. threadx-riscv64 = `heap = true` (the #38
      board — what C.2 lowers to `-D NROS_PLATFORM_HAS_MALLOC`, replacing the
      hand-set `THREADX_GLUE_DEFINES` entry). Catalog parses clean; CLI builds.
- [x] **C.2 — the `-D` lowering (landed).** `cmake/NanoRosCapabilities.cmake`'s
      `nros_board_capability_defines(<board_dir> OUT)` reads `[board.capabilities]`
      from the board's `nros-board.toml` (SSoT) via `file(STRINGS)` and emits the
      matching `NROS_PLATFORM_HAS_*` — no generator, no committed fragment, cmake
      reads the SSoT directly. The threadx-riscv64 overlay's hand-set
      `NROS_PLATFORM_HAS_MALLOC` (the issue-0038 site) is **replaced** by this
      derived value in `THREADX_GLUE_DEFINES`. Because that set is applied to all
      threadx targets (platform cmake), it covers **both** in-tree fixtures and
      scaffolded examples (they build through the same board/platform cmake path).
      Verified: helper unit-checked (threadx heap=true → `-D`, baremetal
      heap=false → none) + a full local `threadx_riscv64 build-fixture-extras`
      builds all 6 zenoh cpp fixtures clean off the *derived* `-D`. The cargo side
      needs no capability lowering — `platform-*` already implies `alloc`.
- [~] **C.2b — RTOS-config agreement check (freertos landed; zephyr deferred).**
      `freertos_capabilities_agree_with_freertosconfig` (in `board_descriptor.rs`)
      cross-checks every FreeRTOS board that co-locates `config/FreeRTOSConfig.h`:
      `configSUPPORT_DYNAMIC_ALLOCATION` ↔ `[board.capabilities] heap`,
      `configUSE_MUTEXES` ↔ `threads` — a merge-gate guard catching the #38-class
      drift (board.toml claims a capability the RTOS config disabled). Zephyr's
      heap/mutex live in per-app Kconfig (`prj.conf`), not a board-local file, so
      they stay config-derived (lower priority — those paths work today).
- [x] **C.3 — reassessed: resolved by design, no risky churn.** The original
      "retire all per-RTOS self-`#define`s" would *break* every platform whose
      C/C++ build doesn't yet receive the capability `-D` (C.2 wired only the
      threadx overlay; posix/freertos/zephyr still rely on their header
      `#define`s). But those self-`#define`s are correct platform **constants**
      (posix always has a heap; freertos always has a mutex) — not a drifting
      dual source. The drift that *did* bite (#38: bare-metal header says
      "no heap" but the board has one) is the only variable case, and C.2 already
      fixed it via the board.toml-driven `-D` opt-in (baremetal.h's
      `NROS_PLATFORM_HAS_MALLOC` gate). So board.toml is authoritative for the
      variable case; the header supplies platform-constant defaults. Full
      retirement (header constants → universal generated `-D`) is low-value purism
      that needs the `-D` wired into *every* platform's C/C++ path first; deferred
      unless a second variable-capability case appears.
- [x] **C.4 — migration lint (landed).** A merge-gate unit test
      (`every_in_tree_board_declares_capabilities` in `board_descriptor.rs`) loads
      the real `packages/boards/*` catalog and fails if any board lacks
      `[board.capabilities]` (relying on inference). All in-tree boards declare
      today; it guards future boards from silently inheriting a wrong default.
- **Acceptance:** flipping a board's `heap`/`atomics`/`threads` in one place
      changes the build everywhere; no capability named in >1 site; the
      threadx-riscv64 `-D` is generated from board.toml, not hand-set.

### 241.D — Deterministic linking (RFC-0042 D3)

> **CURRENT DESIGN (2026-06-14): single shared runtime → [phase-241-d3-single-runtime](phase-241-d3-single-runtime.md).**
> One Rust staticlib per binary (the umbrella) ⇒ `std`/`compiler-builtins`
> monomorphized once ⇒ `--allow-multiple-definition` removable for real. This
> model **subsumes slice 4** below (the dedicated `nros-rmw-cffi-provider` +
> `external-registry` feature are *retired* — one archive ⇒ one `REGISTRY`, plain
> `#[no_mangle]`). The slice-4 design text below is kept for history but is
> **superseded** by the single-runtime doc. D3 bullet 3 (no dup) lands there;
> **bullets 1+2 (one registration path, generated link manifest) + issue 0050
> W3.1 (delete the weak `nros_app_register_backends`) are tracked by
> [issue 0062](../issues/0062-d3-completion-one-registration-path-and-link-manifest.md)**,
> riding on the single-runtime foundation.

> **History (slice-4, 2026-06-13 — RETIRED).** An interim slice-4 cut introduced
> a dedicated `nros-rmw-cffi-provider` archive + an `external-registry` cargo
> feature to define `REGISTRY` once across the C/C++ multi-archive link. It was
> validated, then **superseded by the single-runtime model** (one Rust staticlib
> per binary ⇒ one `REGISTRY` with plain `#[no_mangle]`, no provider/feature). On
> main today there is **no `nros-rmw-cffi-provider` crate and no `external-registry`
> feature**. The dedup motivation + the detailed slice-4 design are preserved in
> git history; the current target is
> [phase-241-d3-single-runtime](phase-241-d3-single-runtime.md) +
> [phase-249](phase-249-one-registration-trigger.md).
- [ ] One registration path: codegen emits the explicit backend-register table,
      used on all platforms; retire the linkme-vs-weak split as a *contract*
      (Q4 lean: explicit table everywhere). **DESIGNED →
      [phase-249](phase-249-one-registration-trigger.md)** (phase-241 W13/R3): the
      uniform explicit `nros_rmw_<backend>_register()` call from the R1 manifest;
      phased P1–P4, P4 deletes the weak default (issue 0062 R2 / 0050 W3.1).
- [x] Generated link manifest (the dispatch half): **DONE — W13/R1** (`RmwDispatch`
      in `resolve_rmw()` → generated `cmake/NanoRosRmwDispatch.cmake`, drift-guarded;
      the W11 synth consumes it). The whole-archive set + archive *order* part is
      still open (rides phase-249 / the NanoRosLink rework).
- [ ] **Remove `--allow-multiple-definition` — rides single-runtime.** The flag
      currently masks legitimate shared-dependency-closure dups (validated: 239
      dups between `libnros_c.a` and the RMW staticlib, ALL from the shared Rust
      closure + the `nros_rmw_cffi_*` C shim — ZERO app/message/transport dups;
      see the validator below). The original slice-3/4 plan (dedup
      `nros-rmw-cffi`'s 7 strong `#[no_mangle]` symbols — `REGISTRY` + the six C
      exports — into ONE archive via a dedicated `nros-rmw-cffi-provider` +
      `nros_rmw_cffi_export!{}` macro, mirroring the platform-cffi split) is
      **SUPERSEDED**: the single shared runtime
      ([phase-241-d3-single-runtime](phase-241-d3-single-runtime.md)) collapses to
      one Rust staticlib per binary, so `REGISTRY` + the cffi C exports are defined
      once with plain `#[no_mangle]` — no provider/feature, flag removable
      directly. Flag removal lands with phase-249's NanoRosLink rework. (The full
      retired slice-4 dedup design is preserved in git history.)
- [~] **Link-closure / duplicate-symbol validator — slices 1+2 landed.**
      `staticlib_duplicate_symbols.rs`: dumps the duplicate defined-globals
      between `libnros_c.a` and the RMW staticlib (via `llvm-nm`), attributes each
      to its embedded v0 crate-id(s), and FAILS on any duplicate from a crate
      outside the shared-dependency closure — i.e. a real ODR violation
      `--allow-multiple-definition` would silently mask. Additive (no link change);
      it turns the blind flag into a scoped, asserted reconciliation.
      **Slice 2 (build-fixture + CI gate):**
      `scripts/build/link-determinism-fixture.sh` builds the host staticlib pair
      (`platform-posix`; the masked dup set is the target-agnostic shared closure,
      so the host pair is a faithful, always-reproducible, SDK-free proxy) into
      `build/link-determinism/` + a stamp; the validator consumes it (falls back to
      any prebuilt cpp `build-zenoh` archives). Wired into `check.yml` after the
      241.A platform-header gate → it is now a HARD PR gate. Next: extend to the
      full link-closure (every FFI-referenced symbol provided by exactly one
      archive), then the dedup + flag removal (slices 3–4) behind `run_e2e`.
- **Acceptance:** the #20 `-u` special-case is gone and threadx-linux+Cyclone
      still links; removing `--allow-multiple-definition` surfaces no real dup
      (the validator already proves the masked set is shared-dep-only);
      a deliberately-dropped lib fails the validator, not `ld`.

### 241.E — Cleanup + docs
- [ ] Flip RFC-0042 sections to `Stable` as each pillar lands (drift rule:
      update ARCHITECTURE.md in the same commit). D1 (one header, via phase-243),
      D2 (capability SSoT), D4 (gate) are landed → flippable now; D3 (linking)
      flips when phase-249 lands.
- [ ] Resolve issue 0042 when D1–D4 acceptances pass; cross-link #27/#36/#38/#20
      as the motivating instances. **D1/D2/D4 landed; only D3 (= phase-249 /
      issue #62) is pending — #42 closes when phase-249 lands.**
- [ ] Update the C/C++ integration docs (RFC-0018/0019, c-api-cmake.md) to point
      at the capability block + manifest.

## Risks / notes

- 241.D is the largest change and the one most able to regress linking on a
  platform the gate doesn't fully exercise — land it last, behind 241.A's gate,
  and validate each platform's e2e (the Phase 240 `run_e2e` dispatch) after.
- 241.B's header collapse touches every platform header; the parity check + the
  241.A gate are the guard rails.
- Keep RFC-0034 (Platform Layer Split & System-ABI Ownership — allocator funnel is
  its first service) and RFC-0035 (RMW vtable ABI — frozen slot table) invariant —
  this phase changes *wiring*, not those ABIs.
