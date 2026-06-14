# Phase 241 — Platform & build determinism

**Implements.** [RFC-0042](../design/0042-platform-build-determinism.md) — one
canonical platform interface, capability-driven config, deterministic linking,
merge-time gate.

**Goal.** End the recurring class of C/C++/Rust build failures (libc/std header
clashes #27/#36/#38, ld single-pass undefined-symbol races #20, silent capability
mismatches) by making the build contract *structural* instead of
convention-enforced. Cross-refs the systemic review in [issue 0042].

**Status.** Planned (2026-06-12).

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

- [ ] **B.3 — unify A's surface into api, then retire A.** Grew from "delete the
      duplicate" into a real ABI unification (A is a *live* complementary surface,
      not legacy) → **carved out as its own phase doc:
      [phase-243](phase-243-platform-abi-unification.md)** (6 waves, exact file:line
      work items + acceptance). Summary: api gains a generic `__atomic` atomics
      inline; the Rust `platform.rs` wrappers re-point to `core::sync::atomic` +
      `clock_us` (collapsing ~20 call sites into one edit); `time_ns`/`sleep_ns` and
      the dead typed-mutex are dropped; POSIX heap funnels through
      `nros_platform_alloc`; A + its 4 sub-headers are deleted and the `nros-c`
      INTERFACE include order flips to api-first. ABI change, every platform — gate +
      parity + full `run_e2e` on a branch.
- [ ] **B.4** — parity assert: extend `c_stub_platform.rs` (or add one) to cover
      the unified canonical header ↔ `lib.rs` once B.3 lands.
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

> **In progress on branch `issue-42-d3-link-determinism`. Slice 4 complete +
> validated.** The **C/C++ multi-archive path** (D3's actual target) is fixed: native
> cpp+c examples link with NO `--allow-multiple-definition` / `--whole-archive`, the
> cpp talker RUNS + publishes (single `REGISTRY`, auto-register fires), and `just
> native build-cpp` links clean.
>
> **Slice-4 regression FIXED via feature-gating the `REGISTRY` def (2026-06-13).**
> The first slice-4 cut made `nros-rmw-cffi` reference `REGISTRY` as an extern
> *unconditionally*, which broke the **pure-Rust firmware path** (`just
> threadx_riscv64 build-fixtures` → `rust-lld: undefined symbol: REGISTRY`): those
> bins link via **cargo, not cmake**, so they never pull the
> `nros-rmw-cffi-provider` archive — and that path never had the multi-archive dup
> problem (one cffi rlib instance) so it should self-define. **Fix:** a new
> `external-registry` cargo feature on `nros-rmw-cffi` gates the `REGISTRY` storage —
> *off* by default → the rlib `#[no_mangle]`-DEFINES its one copy (pure-Rust firmware,
> the NuttX build-std ELF, the dup-symbol negative harness); *on* → it references
> `REGISTRY` as an undefined extern (the provider archive is the sole definer). The
> non-NuttX C/C++ cmake link turns the feature on for every cffi-bundling consumer
> via a passthrough chain (`nros-c`/`nros-cpp` → `nros` → `nros-rmw-cffi`; the
> zenoh/xrce staticlibs → their rmw rlib → `nros-rmw-cffi`; the provider pins it on
> its own cffi dep so `nros_rmw_cffi_export!{}`'s macro def is the lone `REGISTRY`),
> driven by the root `CMakeLists.txt` `NROS_CFFI_EXTERNAL_REGISTRY` guard (set only in
> the non-NuttX branch where the provider is built). **No board-crate changes** — the
> earlier "force-link the provider from ~6 board crates" plan was abandoned in favour
> of this single feature knob (no `#[used]`/`extern crate` hacks, no RTOS allocator
> clash from a provider dep on the boards).
>
> **Validated (2026-06-13):** cffi compiles both feature states; `nm` shows default
> `B REGISTRY` (defined) / `external-registry` `U REGISTRY` (ref) / provider exactly
> one `B`; the host dup-symbol fixture links the 3-archive pair with a *single*
> defined `REGISTRY` (`nros-c.a` + zenoh-staticlib.a contribute 0, provider 1);
> `staticlib_duplicate_symbols` 2/2 pass; `just native build-cpp` GREEN; `just
> threadx_riscv64 build-fixtures` pure-Rust bins link (REGISTRY error gone).
>
> **Separate, pre-existing threadx-riscv64 C-fixture bug surfaced** (now that the
> build progresses past the fixed pure-Rust link): `fixture-0005`
> (`threadx-riscv64-c-zenoh`) fails compiling `nros-platform-threadx/src/{timer,net}.c`
> with `fatal error: nros/platform_{timer,net}.h: No such file or directory` — the
> threadx example/board cmake passes `NROS_PLATFORM_CFFI_INCLUDE=…/nros-platform-cffi/
> include`, but phase-243 moved those canonical headers to `nros-platform-api/include`.
> Independent of slice 4 (no cffi/feature/link involvement); track + fix separately.
>
> **Characterized (2026-06-13):**
> `--allow-multiple-definition` on the threadx/freertos C++ staticlib link masks
> **239 duplicate defined-globals** between `libnros_c.a` and
> `libnros_rmw_zenoh_staticlib.a` — ALL from the shared Rust dependency closure
> (nros-core/serdes/rmw/rmw-cffi, log, core, alloc, heapless, hash32, byteorder)
> + the `nros_rmw_cffi_*` C shim. ZERO application/message/transport dups. So the
> flag masks only legitimate self-contained-staticlib bundling — but note the
> `nros_rmw_cffi_*` C exports are STRONG (`#[no_mangle]`), so removing the flag is
> NOT a contained change: it needs `nros-rmw-cffi` deduped to a single archive
> (see the "Remove `--allow-multiple-definition`" item below for the blocker found
> when the `-u` cmake edit was tried).
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
- [ ] Remove `--allow-multiple-definition` — **strategy partly proven, real
      blocker found (slice 3+4 investigation, 2026-06-13).**
      - The COMDAT/weak duplicates (generics, weak compiler-rt) are NOT the
        problem: a **C-only** link of the 2 archives (`libnros_c.a` +
        `libnros_rmw_zenoh_staticlib.a`) forced via `-u nros_rmw_zenoh_register`,
        lazy (no `--whole-archive`), links with **no** flag — register included,
        single `REGISTRY`. Guarded by
        `host_pair_links_via_u_force_without_allow_multiple_definition`. (The flag
        was historically needed because the real cmake link **whole-archives** the
        backend to keep its `.init_array` register ctor; `--whole-archive` drags in
        every member → the closure's strong defs collide → reproduced as 2462
        multiple-def errors. `-u` keeps the ctor object without that.)
      - **BUT the real C++ link has THREE archives** — `libnros_c.a` +
        `libnros_cpp.a` + the RMW staticlib — and all three statically bundle
        `nros-rmw-cffi`, whose C exports (`nros_rmw_cffi_register_named`,
        `nros_rmw_cffi_set_custom_transport`, …) are **STRONG** (`#[no_mangle]`),
        not COMDAT. Converting the root `CMakeLists.txt` zenoh link to `-u` (drop
        whole-archive + flag) was tried and **fails the native cpp link**:
        `multiple definition of nros_rmw_cffi_set_custom_transport` (libnros_cpp.a
        vs the RMW staticlib). So lazy linking does NOT dedup these — the flag is
        load-bearing for the strong cffi C exports.
      - **Therefore slice 4 (drop the flag) REQUIRES the dedup first:**
        `nros-rmw-cffi` must be defined in exactly ONE archive, not bundled into
        each staticlib (`nros-c`, `nros-cpp`, the RMW staticlib). Options: a single
        shared `nros-rmw-cffi` archive the others reference as undefined (hard with
        Rust `crate-type=["staticlib"]`, which always bundles its closure), or
        localising the cffi exports in all-but-one archive via `objcopy
        --localize-symbol` (UNSAFE for the stateful `REGISTRY` — would split the
        registry → #48 `NoBackend`), or collapsing nros-c+nros-cpp+RMW into one
        staticlib. This is the architectural core of D3 and needs its own design +
        run_e2e. The cmake `-u` edit was reverted (it broke the cpp link); the
        validator + the C-only host proof stand as the guardrails for it.
        **See "Slice 4 design" below.**

#### 241.D slice 4 — the `nros-rmw-cffi` dedup design (2026-06-13)

**Problem (precise).** The same `nros-rmw-cffi` rlib is statically bundled into
three archives that all reach the final link: `libnros_c.a`, `libnros_cpp.a`, the
RMW staticlib (`libnros_rmw_zenoh_staticlib.a` / xrce). Its C exports
(`nros_rmw_cffi_register{,_named}`, `_lookup`, `_registered_names`,
`_walk_init_section`, `_set_custom_transport`) and the registry static `REGISTRY`
are **strong** (`#[unsafe(no_mangle)]`, `lib.rs:976/1196/…`). Three strong
definitions → `--allow-multiple-definition` is the only thing letting the link
proceed. Invariant: **`REGISTRY` must resolve to exactly ONE runtime instance** —
the backend registers into it and `Executor::open` reads it; two instances =
`NoBackend` (#48 class).

**Precedent.** The *platform*-cffi split is already done: `libnros_c.a` references
`nros_platform_*` as **undefined** (`U nros_platform_log_write`), supplied by a
separate `libnros_platform_<plat>.a`. The rmw-cffi split is the missing analogue
(the `NanoRosLink.cmake` header even predicts it: "once the RMW-cffi canonical-ABI
binary split lands, the bodies swap to linking three separate archives").

**What actually collides (narrowed).** Under `--whole-archive` only the **strong**
symbols collide; the Rust-mangled cffi/closure symbols are `linkonce`/COMDAT and
dedup themselves. The strong colliders are exactly nros-rmw-cffi's
`#[unsafe(no_mangle)]` set: `REGISTRY` (the stateful registry static, `lib.rs:976`)
+ the six C exports (`nros_rmw_cffi_register{,_named}`, `_lookup`,
`_registered_names`, `_walk_init_section`, `_set_custom_transport`). So the fix
only needs **those ~7 symbols defined once**, not a whole-crate refactor.

**Rejected — weak symbols.** Weakening the 7 (e.g. `llvm-objcopy --weaken-symbol`)
links clean with one `REGISTRY` in a host probe, but weak linkage is **ordering-
/GC-/ODR-fragile** (which weak def wins depends on archive order + `--gc-sections`;
a future change can silently flip the chosen `REGISTRY`). Not used.

**Rejected — localize / `--exclude-libs`.** Making the cffi symbols file-local in
all-but-one archive **splits `REGISTRY`**: each archive's localized code binds to
its own copy → the backend registers into a different registry than
`Executor::open` reads → #48 `NoBackend`. Unsafe.

**Recommended — define-once via the platform-cffi export-macro pattern.** The
*platform*-cffi split already solves this exact shape, and the way it does it is
the key: `nros-platform-cffi` **never defines** `nros_platform_*` inline — it only
**declares** them (`unsafe extern "C" {}`) and ships `nros_platform_export_*!`
**macros**; exactly ONE port crate (`nros-platform-posix`) *invokes* the macro, so
the definition is emitted in exactly one archive and everyone else references it
undefined. Mirror this for rmw-cffi — concrete implementation:
  - **`nros-rmw-cffi` becomes def-less.** Replace `#[unsafe(no_mangle)] pub static
    REGISTRY = Registry::new()` with an **`unsafe extern "C" { pub static REGISTRY:
    Registry; }`** declaration. The seven `#[unsafe(no_mangle)]` fns
    (`nros_rmw_cffi_register{,_named}`, `_lookup`, `_registered_names`,
    `_set_custom_transport`, `_walk_init_section`) keep their bodies but as plain
    `pub fn …_impl(…)` (NOT `no_mangle`); they + the in-crate Rust API
    (`resolve_backend`, `default_vtable`, `backend_registered`, `get_vtable`) all
    reference the **extern** `REGISTRY`. So the cffi rlib, bundled in every consumer,
    carries ZERO strong `#[no_mangle]` defs — only undefined refs.
  - **`nros_rmw_cffi_export!{}` macro = thin wrappers** (small, not a 200-line
    code-move): it emits `#[unsafe(no_mangle)] pub static REGISTRY: $crate::Registry
    = $crate::Registry::new();` + one `#[unsafe(no_mangle)] pub … fn
    nros_rmw_cffi_<x>(…) { $crate::<x>_impl(…) }` wrapper per export. The logic
    stays in `$crate`; only the strong symbol + a one-line delegation live in the
    macro.
  - **Provider = a NEW dedicated `nros-rmw-cffi-provider` staticlib crate**, NOT
    nros-c. *Why not nros-c:* `libnros_c.a` AND `libnros_cpp.a` both bundle the cffi
    rlib independently (verified: both define `nros_rmw_cffi_register_named`), and
    nros-cpp deps nros-rmw-cffi directly — so any crate that gets bundled into >1
    archive (nros-c, nros-cpp) can't host the macro without re-duplicating. The
    provider must be its own archive linked exactly once — precisely the
    `nros-platform-posix` shape. The provider crate's `lib.rs` is one line:
    `nros_rmw_cffi::nros_rmw_cffi_export!{}`.
  - **cmake:** add the provider archive to `NanoRos` (mirror the
    `NrosPlatformPosix` link) so every final link pulls it exactly once, then drop
    `--allow-multiple-definition` (root `CMakeLists.txt` zenoh/xrce/cyclone arms +
    the secondary `nros-c/cmake/NanoRosLink.cmake` site). The NuttX/ESP cargo-FFI
    path links the provider via a normal cargo dep on the FFI crate.
  - nros-cpp + the RMW backends keep using the cffi **Rust API unchanged** — they
    just no longer emit the strong defs (only the provider does).

**Why the macro, not a `provide` cargo feature — the unification trap.** A
positive `provide`/`define-c-abi` *feature* on `nros-rmw-cffi` would be **unified
ON across the whole graph** by cargo (if nros-c turns it on, every nros-rmw-cffi
instance gets it) → the defs reappear in every archive → back to the dup. A macro
*invocation* is a per-crate source item, not a feature, so only the crate that
writes the call emits the defs. This is precisely why the platform split uses an
export macro, and why slice 4 must too.

**Blast radius.** Bounded to `nros-rmw-cffi` (the 7 symbols → extern decls + one
export macro) + one provider call site + the cmake flag-line removals. The cffi
**Rust API and the backends are untouched** (no C-ABI refactor — the earlier fear
was overscoped; only `REGISTRY` + the 6 C fns move, and the Rust API already goes
through `REGISTRY` which simply becomes an external symbol).

**Open questions for the implementation run.** (a) *Resolved* — provider is a
dedicated `nros-rmw-cffi-provider` archive (nros-c/nros-cpp are both multi-archive,
so neither can host the macro). (b) The Rust API operates on `REGISTRY` directly
(via `default_vtable`/`resolve_backend`), NOT through the C-export fns, so making
`REGISTRY` extern is enough; the C fns are only for C callers and live solely in
the provider. (c) the threadx board + zpico `--allow-multiple-definition` are a
DIFFERENT class (intentional startup.c memset/memcpy overrides) — leave them; (d)
validate across every linker (lld/arm/riscv) + bare-metal-explicit-register vs
posix `.init_array` register paths via `run_e2e`. The `staticlib_duplicate_symbols`
gate guards that no new strong dup sneaks in while this lands.
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
      update ARCHITECTURE.md in the same commit).
- [ ] Resolve issue 0042 when D1–D4 acceptances pass; cross-link #27/#36/#38/#20
      as the motivating instances.
- [ ] Update the C/C++ integration docs (RFC-0018/0019, c-api-cmake.md) to point
      at the capability block + manifest.

## Risks / notes

- 241.D is the largest change and the one most able to regress linking on a
  platform the gate doesn't fully exercise — land it last, behind 241.A's gate,
  and validate each platform's e2e (the Phase 240 `run_e2e` dispatch) after.
- 241.B's header collapse touches every platform header; the parity check + the
  241.A gate are the guard rails.
- Keep RFC-0034 (allocator funnel) and RFC-0035 (vtable slots) invariant — this
  phase changes *wiring*, not those ABIs.
