# Phase 248 — Platform/RMW-agnosticism convergence

**Goal.** Converge the code onto the target architecture audited in **issue #60**:
core packages + user C/C++/Rust libraries are platform- AND RMW-agnostic (carry
only functional features); RMW + platform reached purely via the vtable ABI;
hardware specifics in board packages; workspace RMW/platform selection
config-file-driven (RFC-0004/0031). This phase closes the convergence debt; it
does NOT redesign — the target is already RFC-0004 (config) + RFC-0031 (RMW
selection) + the platform vtable (`nros-platform-api`/`-cffi`) + the RMW vtable
(`nros-rmw-cffi`).

**Status.** Proposed 2026-06-14. Implements issue #60. **Wave 1 COMPLETE**
(C1 boards, C2 nros-node, C3.1 nros-c/nros-cpp, C4 docs — landed + integration-
verified: umbrella builds zenoh+posix; nros-node 162+5 / nros-rmw 44 /
cyclonedds 15 pass; native cross-process pub/sub e2e green). **Wave 2 IN
PROGRESS (2026-06-14): C5b + C5-plat DONE (codegen lowers RMW+platform to the
board); C6a/b/c DONE; C6-TAIL DONE — every hand-written consumer + the codegen
is off `nros/{rmw,platform}-*`, all three repo-wide gates clean. C5a DONE
(`01e0ffc62` — all deploy boards self-link + register their RMW). C5c DONE
(2026-06-14) — `nros` is RMW + platform AGNOSTIC: removed `rmw-{zenoh,xrce,
cyclonedds}` + `platform-{posix,bare-metal,freertos,nuttx,threadx,cffi,orin-spe}`
+ the optional backend deps + the umbrella force-link statics; nros-c/nros-cpp
route platform via a direct `nros-platform` dep (full-agnostic choice, preserving
D3's single-runtime backend bundle).** **C3.2 SUPERSEDED by 241.D3-rev**; issue
#61 closed `wontfix`. **Phase essentially complete** — validated: nros builds
agnostic (std/no_std) + default, nros-c (zenoh single-runtime)/nros-cpp/native/
px4-xrce build, nros-c 71 + cli generate 21 pass, nm proves the native binary
self-registers zenoh (app-owned force-link, independent of nros); embedded path
covered by C5a. **C7 DONE (Method A, 2026-06-14): `platform-zephyr` DELETED** —
`nros` now carries ZERO `platform-*` features. The Zephyr entry macro
`zephyr_component_main!` stays in `nros` as framework API (re-gated `rmw-cffi`-only,
same category as `nros::main!`); the `wait_network` helper moved to `nros-platform`
(step 1). **EMBEDDED RUNTIME SMOKE — FreeRTOS GREEN (2026-06-15).** `just freertos
build-examples` cross-builds + links the board-driven firmware on thumbv7m (after
fixing a C6f gap: `examples/fixtures.toml` still passed `features=["rmw-zenoh"]` to
the now-board-driven freertos rust components — `10bd115b0`). On the patched
qemu-system-arm 11.0.0 (`just qemu setup-qemu`), `freertos_run_plan_runtime` boots
all 6 entries (talker/listener/services/actions) through `BoardEntry::run` — each
self-registers its RMW (C5a, no `nros` force-link), `Executor::open` connects to the
slirp zenohd, and reaches `Application setup complete`. (6/6 pass run serially; 2
flake under 6 parallel QEMU instances — pre-existing harness concurrency, not a
registration regression.) Proves C5c+C7 agnostic `nros` + board self-register at
RUNTIME on embedded. FreeRTOS is the representative full-board+zenoh path.

**EMBEDDED RUNTIME SMOKE — EXTENDED (2026-06-15, the handoff list run).** The
remaining harness cells now run; the agnostic board-self-register pattern is
runtime-proven on **four** independent embedded paths covering both
registration-fire mechanisms (hosted-walks-`.init_array` AND
bare-metal-startup-does-not):
- **threadx-rv64** ✅ `threadx_riscv64_qemu` 2/2 — two-QEMU CycloneDDS pub/sub
  over NetX/virtio-net boots + exchanges (NetX board path).
- **nuttx** ✅ `nuttx_qemu` 14/14 (kernel boots; 12 C/C++ build-checks) +
  `integration_nuttx` smoke.
- **baremetal (MPS2-AN385, smoltcp)** ✅ `baremetal_run_plan_runtime` — boots
  reset → `BoardEntry::run` → `Executor::open`. Exercises the **canonical
  bare-metal registration path** (`__register_linked_rmw` on `target_os="none"`,
  the path that does NOT walk `.init_array` — the phase-249 P4 gate). Required a
  fixtures wiring fix: `qemu-baremetal-main-e2e` was referenced by the test but
  staged by no lane (no `fixtures.toml` entry → test skipped→FAILed); added the
  manifest entry so `just qemu build-fixtures` stages it.
- **esp32-c3 (riscv32)** — build/detection ✅ (4/8); live pub/sub e2e remains
  **deferred-red** (pre-existing, per the handoff): boots 80–157 s then misses
  messages (networking/timing flake, NOT registration — firmware reaches its
  banner), plus a `workspace_entry_e2e` staging gap (the `esp32_entry` workspace
  fixture isn't built by `just esp32 build-fixtures`, which stages only the
  `esp_idf_bringup` app). Tracked with the embedded-harness residual; esp32 uses
  the identical board-self-register pattern proven on the other four.
- **zephyr** — still BLOCKED on west/SDK (#58/#59).

Net: the C5a/C5c/C7 agnostic-`nros` + board-self-register design is runtime-green
on freertos + threadx-rv64 + nuttx + baremetal — enough to gate the
[phase-249](phase-249-one-registration-trigger.md) registration-trigger
retirements (migrate-before-delete) without esp32/zephyr.

> **HANDOFF (remaining embedded smoke — for the next agent).** The agnostic
> board-self-register pattern is runtime-proven on FreeRTOS; the rest use the
> identical pattern, just not yet run here. To extend coverage:
> 1. **Provision the patched QEMU once:** `just qemu setup-qemu` (prebuilt fetch,
>    fast) → `build/qemu/bin/qemu-system-arm`. Export `QEMU_SYSTEM_ARM` to it for
>    the tests. (riscv64/xtensa QEMU + arm-none-eabi-gcc are already on PATH.)
> 2. **Baremetal (smoltcp, MPS2-AN385):** needs the compile-check fixture staged
>    — `just build-test-fixtures` (stages `qemu-baremetal-main-e2e` +
>    `freertos_firmware` `.compile-ok` stamps; heavier than `just qemu build`).
>    Then `cargo test -p nros-tests --test baremetal_run_plan_runtime --
>    --test-threads=1` (serial — the harness flakes N QEMU instances in parallel).
> 3. **threadx (riscv64):** `cargo test -p nros-tests --test threadx_riscv64_qemu`
>    (+ stage its fixture). **nuttx:** `--test nuttx_qemu` / `integration_nuttx`.
>    **esp32:** `--test esp32_emulator` (qemu-system-xtensa). Each: build the
>    board-driven entry, boot, assert it self-registers + reaches setup-complete.
> 4. **Zephyr:** BLOCKED on west/SDK (#58/#59) — also closes the C7 runtime gate
>    (the `zephyr_component_main!` `rust_main` wiring is cargo-check-only today).
> 5. **Harness flake (optional):** the `*_run_plan_runtime` tests starve under
>    parallel QEMU — a per-test serialization or QEMU-instance cap would let them
>    run multi-threaded. Pre-existing, not an agnosticism issue.

> **Cross-phase note (2026-06-14).** C5a's "boards self-link + register their RMW"
> (the app-owned force-link + explicit register) is the bare-metal half of the ONE
> registration trigger that **[phase-249](phase-249-one-registration-trigger.md)**
> (RFC-0042 §D3 bullet 1 / phase-241 W13/R3) is unifying across C/C++ + pure-Rust +
> embedded: one explicit `nros_rmw_<backend>_register()` call from the R1 dispatch
> manifest, retiring the linkme slice + `.init_array` ctors + the weak
> `nros_app_register_backends` default. Board `entry.rs` register sites are
> phase-249 P1's surface — do not re-introduce a linkme/ctor registration path.

**Priority.** P2 — architectural hygiene; not blocking features, but every new
platform/RMW today pays the leakage tax (feature matrices, concrete-backend
deps in core).

**Depends on.** RFC-0004, RFC-0031, RFC-0005/0006 (feature axes), the platform
vtable (`nros-platform-api` already exposes `wake_*`, alloc, spin ops), the RMW
vtable (`nros-rmw-cffi`).

## How to run this phase (parallel clusters)

Work is partitioned by **crate ownership** so clusters edit disjoint files and
run on separate agents without contention. Three waves by dependency:

```
WAVE 1 (parallel — fully independent, dispatch all at once):
   C1 Boards        C2 nros-node       C3 nros-c/nros-cpp     C4 Docs/RFC
        │                │                    │ (src→vtable)        │
        └────────────────┴──────────┬─────────┴─────────────────────┘
                                     ▼
WAVE 2 (after W1 vtable ops + selection-model land):
                            C5 nros umbrella + selection model (keystone)
                                     │
                          (C3 phase-2: retire nros-c/nros-cpp features)
                                     ▼
WAVE 3 (after C5):
                            C6 example node pkgs (strip feature matrices)
```

**Parallel-safe set (Wave 1): C1, C2, C3-phase1, C4** — disjoint crate ownership
(boards / nros-node / nros-c+nros-cpp / docs). Dispatch concurrently.
**C5** is the keystone (changes the selection model) — single coordinated effort
after Wave 1. **C6** + **C3-phase2** follow C5.

Each cluster: run `just ci` (or the scoped build/test it names) before handing
back; nightly `cargo +nightly fmt`; restore incidental cbindgen/Cargo.lock churn.

---

## C1 — Boards: gate concrete RMW optional (#60 T4)

**Owns:** `packages/boards/{nros-board-native,nros-board-rtic-mps2-an385,
nros-board-rtic-stm32f4,nros-board-embassy-stm32f4}/Cargo.toml`.
**Blocked-until:** none (Wave 1).

- [x] **DONE.** All 4 boards' `nros-rmw-zenoh` dep is now `optional = true`,
      wired into a default-on `rmw-zenoh` feature (`["dep:nros-rmw-zenoh"]`); the
      src `register()`/`extern crate` references are `#[cfg(feature="rmw-zenoh")]`-
      gated. The per-dep `features=[platform-*, ros-humble]` activate only with
      the optional dep.
- **Acceptance:** DONE — all 4 build default + no-zenoh; with the feature off,
  `nros-rmw-zenoh` is NOT compiled (verified via cargo-metadata + artifact
  inspection on the cross targets). All 4 deps `optional = true`.

## C2 — nros-node: RMW + platform decoupling (#60 T1 + T3-node)

**Owns:** `packages/core/nros-node/` (all), the cyclonedds descriptor-registration
seam in `packages/dds/nros-rmw-cyclonedds` + `nros-rmw`/`nros-rmw-cffi` (the
generic hook), and any new vtable op added to `nros-platform-api`/`-cffi`.
**Blocked-until:** none (Wave 1) — the platform `wake_*` vtable already exists in
`nros-platform-api`; this routes through it.

- [x] **T1 — drop unconditional `nros-rmw-cyclonedds` dep.** Today `nros-node`
      links it because `MessageForRmw` + `cyclonedds_register` reference it
      unconditionally (cyclone's type-descriptor registration leaked into core).
      Make the descriptor-registration a GENERIC vtable hook (the RMW that needs
      per-type descriptors registers via `nros-rmw-cffi`, not a named-backend dep
      on the core executor). Drop the `nros-rmw-cyclonedds` + `-sys` deps from
      `nros-node/Cargo.toml`.
- [x] **T3 — route platform wake/alloc/spin through the vtable.** Remove the
      `#[cfg(feature="platform-{zephyr,freertos,nuttx,threadx}")]` branches in
      `executor/{node_wake,wake_alloc,spin}.rs`; call the `nros-platform-api`
      `wake_*` / alloc / spin ops generically (the vtable already defines them).
- [x] Delete the now-unused `platform-*` feature DECLARATIONS from
      `nros-node/Cargo.toml`. Fix the stale "Phase 104.A removed" comment.
- **Acceptance:** DONE. Generic descriptor seam in `nros-rmw`
  (`type_descriptor.rs`: `set_type_descriptor_registrar` /
  `register_type_descriptor`); cyclone self-installs from `nros-rmw-cyclonedds`
  (+ `-sys` via its `RMW_INIT_ENTRIES`); `nros-node` dropped the
  `nros-rmw-cyclonedds[-sys]` deps (`__cyclonedds-link` now a pure marker).
  Platform wake/alloc/spin select the kernel primitive at RUNTIME
  (`wake_storage_size()==0` probe) — no `platform-*` cfg. grep empty; nros-node
  162+5, nros-rmw 44, cyclonedds 15 pass; no_std + umbrella build.
  **C5 hand-off:** 8 `platform-* = []` INERT no-op shims remain in
  `nros-node/Cargo.toml` only because `nros/Cargo.toml` forwards
  `nros-node/platform-*` — C5 must delete those 8 shims TOGETHER with the
  matching `"nros-node/platform-*"`/`"nros-node/platform-udp"` forwarding in
  `nros`, and add a `-sys` rlib keep-alive (`extern crate
  nros_rmw_cyclonedds_sys as _;`) in the umbrella (the old
  `__FORCE_LINK_CYCLONEDDS_SYS` left nros-node).

## C3 — nros-c / nros-cpp: platform decoupling + feature retirement (#60 T2/T3 C/C++)

**Owns:** `packages/core/nros-c/` + `packages/core/nros-cpp/` (all).
**Blocked-until:** phase-1 none (Wave 1); phase-2 after **C5**.

- [x] **Phase 1 (Wave 1) — platform impls behind the vtable. DONE.** Collapsed
      the per-platform `#[global_allocator]` modules into one `platform_alloc`
      gated `global-allocator` (routes through `nros_platform_alloc/_dealloc`);
      rewrote the zephyr-only critical-section to `platform_critical_section`
      gated `critical-section` (calls `nros_platform_critical_section_acquire/
      _release`); extracted the no_std panic handler. Same on `nros-cpp/src`. No
      `#[cfg(feature="platform-*")]` left in either src; no new platform-api op
      needed (vtable ops already existed). nros-c tests 71 pass; both build green.
- [~] **Phase 2 — retire features. SUPERSEDED by 241.D3-rev (2026-06-14).**
      C3.2 (`d44a555c1`) dropped `nros-c`/`nros-cpp`'s `platform-*` +
      concrete-`rmw-*` features + backend deps to make them fully agnostic. It
      was **DROPPED during the rebase onto a main that had landed Phase
      241.D3-rev** (single-runtime umbrella), which deliberately RE-COUPLES the
      C/C++ libs to ONE board-selected backend rlib (`rmw-zenoh = ["rmw-cffi",
      "dep:nros-rmw-zenoh"]` + `src/rmw_backend.rs` force-link) to kill the
      multi-staticlib double-cffi-instance hazard. **Reconciliation:** the C/C++
      **staticlib root** is the sanctioned place to bundle the backend (one
      `libnros_c.a` = C ABI + cffi + backend → one `std`/`REGISTRY`/zenoh-pico);
      its `platform-*`/`rmw-*` features are the **board-driven selectors** for
      that bundled backend (the #60 staticlib-root exception), NOT user leakage.
      Phase-1's agnostic `src/lib.rs` (no `platform-*` cfg) STANDS — D3 carries
      it via the `global-allocator`/`critical-section` vtable routing. Net: the
      C/C++ libs follow D3 on `main`; only the umbrella `nros` + the example/
      codegen layer (C5/C6) go feature-agnostic. **Issue #61 closed `wontfix`**
      (its premise — features removed — is void; they remain on `main`).

  *(superseded checklist item kept for history:)*
- [ ] **Phase 2 (Wave 2, after C5) — retire features.** Drop `platform-*` +
      concrete-`rmw-*` features + optional concrete-backend deps
      (`nros-rmw-zenoh`, `nros-rmw-xrce-cffi`) from `nros-c`/`nros-cpp/Cargo.toml`;
      keep only functional features (`std`/`alloc`, `rmw-cffi` = the vtable,
      `param-services`, ROS edition). RMW/platform now selected via the model C5
      establishes.
- **Acceptance:** C builds (`--features rmw-cffi,...` per AGENTS.md) green; no
  `platform-*` cfg in `nros-c`/`nros-cpp` src; phase-2: no `platform-*`/concrete
  `rmw-*` features or concrete-backend deps in either Cargo.toml.

## C4 — Docs/RFC: formalize the agnostic-core principle (#60 docs)

**Owns:** `docs/design/` (RFC edits) + this phase doc + issue #60.
**Blocked-until:** none (Wave 1).

- [x] Made the **agnostic-core + vtable-seam + config-selection** principle
      explicit: added the **Agnosticism contract** to ARCHITECTURE §2 (names the
      crates that must NOT carry `platform-*`/`rmw-*`, the vtable seams they use
      instead, config-driven selection) + cross-links RFC-0004/0005/0006/0031 +
      issue #60. RFC-0006 (the vtable interface) gains an "enforcement role" note.
- [x] CI-guard idea noted (a `just` grep over core/user-lib `Cargo.toml`s for
      forbidden `platform-*`/`rmw-*` features) — specced in ARCHITECTURE §2 as a
      post-convergence enforcement; implementation is an optional follow-up.
- **Acceptance:** DONE — ARCHITECTURE §2 + RFC-0006 state the contract; no code
  change.

## C5 — nros umbrella + selection model (keystone, #60 T2) — WAVE 2

**Owns:** `packages/core/nros/` (all) + the workspace/board selection wiring
(`packages/cli` codegen/board-resolve + board crates' RMW forwarding, as needed).
**Blocked-until:** **C1 + C2 + C3-phase1** (vtable ops + optional-RMW boards).

- [x] Establish the **config/board-driven RMW+platform selection** so the
      `nros` umbrella no longer needs `rmw-*`/`platform-*` features: the board
      crate (selected by entry `[package.metadata.nros.entry] deploy=` /
      `system.toml` `[deploy.<id>]`) brings the concrete RMW + platform backend
      into the link graph; `nros` consumes only the vtable shims. RMW value from
      `system.toml` `[system].rmw` / `[deploy.<id>].rmw` (RFC-0031) drives which
      backend the board/build links.
- [x] Retire `rmw-{zenoh,xrce,cyclonedds}` + `platform-*` features + the optional
      concrete-backend deps from `nros/Cargo.toml`; remove the `platform-*` cfg
      branches in `nros/src/lib.rs` (route through vtable). Fix the stale
      "Phase 104.A removed" comment.
- **Acceptance:** a native + an embedded example build + run selecting RMW via
  config/board only (no `nros/rmw-*`/`platform-*` feature anywhere in the
  graph); `nros/Cargo.toml` carries only functional features. Full pubsub E2E
  (zenoh + xrce + cyclone) still green via config selection.

## C5 EXPANDED — full board-driven selection (decided 2026-06-14)

Maintainer chose the **strict** reading of expectation #1: the `nros` umbrella is
fully agnostic — it must NOT carry `rmw-*`/`platform-*` features or concrete-
backend deps. The **board crate becomes the RMW+platform selection point** (it
brings the concrete backend + platform impl into the link graph + carries the
backend force-link statics); codegen lowers `system.toml` `[system].rmw` /
`[deploy.<id>].rmw` to the **board's** `rmw-X` feature, not an `nros` feature.
This SUPERSEDES the C4-contract escape hatch ("umbrella may carry features as
lowering target") — update that line in ARCHITECTURE §2 to name the board crate
as the lowering target. Likely an **RFC-0031 amendment** (lowering target moves
nros-feature → board-feature) — land that as part of C5b.

The move is large + cascades (every example/entry/fixture + the codegen). Keep
the tree GREEN by sequencing additively — drop nros's features LAST, only after
every consumer is migrated. Sub-clusters + sub-waves:

**C5.1 — DONE** (C2 hand-off cleared; nros-node carries no platform-* surface;
cyclone keep-alive moved to nros). See above.

**Wave 2a (foundational — establishes the board-as-selection-point, ADDITIVE so
nros keeps its features for now):**
- [~] **C5a — Selection mechanism in boards. PATTERN PROVEN (native zenoh).**
      `nros-board-native` now carries a `#[cfg(feature="rmw-zenoh")]`
      `__FORCE_LINK_ZENOH` static (mirrors nros); nm-verified that the BOARD
      (features=["rmw-zenoh"]) — with `nros` built WITHOUT `rmw-zenoh` — pulls +
      self-registers zenoh (`RMW_INIT_ENTRIES` reaches the binary via the board,
      no nros force-link). Additive: nros keeps its features. Most cross boards
      were already self-sufficient (C1 added `#[cfg(rmw-zenoh)]` boot-path
      `register()` calls). **Remaining C5a per-board work (gates C5c — must be
      done so NO board forwards `nros/rmw-*`):**
      - `nros-board-freertos` (+ `-mps2-an385-freertos` via it) still forwards
        `rmw-zenoh = ["nros/rmw-zenoh"]`; registration rides on
        `nros::__register_linked_rmw()`. Move to board-owned register (couples
        with C5b's macro change).
      - Pure-marker boards (`rmw-zenoh = []`, zenoh via the C TU / zpico-sys):
        `mps2-an385`, `stm32f4`, `esp32s3`, `fvp-aemv8r-smp`, `s32z270dc2-r52` —
        add the optional Rust `nros-rmw-zenoh` dep + gated register for
        board-owned linking.
      - XRCE: `nros-board-mps2-an385` (`xrce-transport`) — add `__FORCE_LINK_XRCE`
        + board register. CycloneDDS: `fvp-aemv8r-smp`, `s32z270dc2-r52`
        (`rmw-cyclonedds` markers) — add optional `-sys` dep + force-link.
      - `nros-board-esp32-qemu`: ungated `register()` (can't build rmw-zenoh-off)
        — minor C1 nit to gate.
      Owns: `packages/boards/*` + the force-link block in `nros/src/lib.rs` (READ
      only). Platform-axis board debt (e.g. `nros-board-posix` enables
      `nros/platform-posix`) is Tier-3, handled with C3.2/C5c.
- [x] **C5b — Codegen lowers to the board feature + RFC-0031 amendment. DONE.**
      The RMW lowering target moved from `nros/rmw-X` to the **board crate's**
      `rmw-X` feature: `render_platform_dependencies`/`board_dep` (generate.rs) +
      `scaffold_rust` now put `features=["rmw-X"]` on the board dep, `nros` gets
      only the `rmw-cffi` vtable. RFC-0031 amended (board = Rust lowering target).
      Tests green: nros-cli-core lib 376, orchestration_generate 21,
      cargo-nano-ros 46. **Follow-ups (→ C5c):** (1) PLATFORM-axis lowering
      (`nros/platform-*` → board) — **DONE (C5-plat, 2026-06-14):** boards bring
      `nros-platform { features=["platform-<rtos>"] }` directly (no board forwards
      `nros/platform-*`), codegen drops `nros/platform-X` for board-backed
      entries; RFC-0031 amended for both axes. cli 376+21+46 still pass.
      (2) Crate-less native/posix + zephyr
      orchestration still link via a direct `nros-rmw-*` path dep + explicit
      `register()` in `render_backend_register_fn` (no `nros-board-*` crate to
      carry the feature) — moving them needs `nros-board-native` in the board
      descriptor/catalog, dropped in C5c after C6 migration. Owns: `packages/cli`
      + RFC-0031.

**Wave 2b (migration — parallel by consumer group, AFTER 2a):**
- [x] **C6a — Migrate Rust workspace + native examples** off `nros/rmw-*`/
      `nros/platform-*`; select via board + `system.toml`. (#60 T5)
- [x] **C6b — Migrate C/C++/mixed workspace examples** (drop `DEPLOY native` +
      CMake rmw/platform pins → board/config). (#60 T5)
- [x] **C6c — Migrate embedded examples** (qemu-*/stm32f4 node pkgs). (#60 T5)
- [~] **C3.2 — Retire nros-c/nros-cpp features. SUPERSEDED by 241.D3-rev** — the
      C/C++ staticlib root bundles one board-selected backend (single-runtime
      umbrella), so it keeps its `platform-*`/`rmw-*` selectors. See the C3
      section. Owns: nros-c + nros-cpp.

**Wave 2c (cleanup — AFTER every consumer migrated):**
- [x] **C5c — Drop nros's `rmw-*`/`platform-*` features + concrete-backend deps.
      DONE (2026-06-14, `52a85d6ff`).** Removed `rmw-{zenoh,xrce,cyclonedds}` +
      `platform-{posix,bare-metal,freertos,nuttx,threadx,cffi,orin-spe}` +
      `platform-udp`/`xrce-udp`/`xrce-serial`/`link-tls`/`link-custom` features,
      the optional `nros-rmw-{zenoh,xrce-cffi}` / `nros-rmw-cyclonedds-sys` deps,
      the `?/` backend forwards, and the `__FORCE_LINK_*` statics +
      `__register_linked_rmw` body from `nros`. nros now consumes only
      `nros-rmw-cffi` + `nros-platform` vtable seams. **Full-agnostic path
      (maintainer choice):** nros-c/nros-cpp route platform via a direct
      `nros-platform[platform-X]` dep (not `nros/platform-X`), keeping D3's
      single-runtime backend bundle. **Residual:** `platform-zephyr` stays on
      nros gating the Zephyr entry-point scaffolding (relocation = Tier-3
      follow-up, gated on a green Zephyr build #58/#59). Validated at build +
      link-symbol + test level (native/C-C++/codegen); embedded via C5a. Owns:
      `nros/` + `nros-c`/`nros-cpp` platform reroute.
      **C6-TAIL CONSUMER MIGRATION COMPLETE (2026-06-14) — gates now clean.**
      Every hand-written consumer + the codegen is off `nros/{rmw,platform}-*`:
      px4, zephyr rust + `zephyr_entry`, native/rust (board-less posix), px4,
      baremetal (inert `platform-bare-metal` dropped), nuttx + riscv64-threadx
      (re-homed to a direct `nros-platform[platform-X]` dep), nros-bench,
      nros-tests bins + harness, the bridge demo, templates, and the fixtures
      (multi_pkg_workspace_freertos / orchestration_tiers_freertos libs,
      one_dep_component_pkg, n_board_agnostic posix_entry/freertos_entry +
      shared_node_pkg), and book/rustdoc-driver. The CODEGEN
      (`generated_default_features` + `render_platform_dependencies`) no longer
      lowers the platform axis onto `nros` (commit `refactor(codegen): C5c`).
      All three gates clean repo-wide: (1) `git grep 'nros/\(rmw\|platform\)-'`
      (excl `rmw-cffi`/comments) empty; (2) no `platform-*` on any `nros` dep
      line; (3) no concrete `rmw-*` on any `nros` dep line. The pattern per
      consumer class: board-less posix app keeps `nros-platform-cffi[posix-c-port]`
      (symbol source) + app-owned `nros-rmw-*` dep + force-link; RTOS standalone
      re-homes to `nros-platform[platform-X]`; libs just drop the feature (the
      entry/firmware brings platform); board-backed entries get platform+rmw from
      the board crate.

      **C5c (the actual nros-feature DROP) now blocked ONLY on C5a per-board
      register, NOT on consumer edges.** Dropping `nros`'s `rmw-{zenoh,xrce,
      cyclonedds}` features + concrete deps empties `__register_linked_rmw()`
      (`nros/src/lib.rs`) — the registration path the `nros::main!` macro calls
      for **linkme-blind targets** (Zephyr native_sim `target_os="none"`,
      bare-metal, NuttX, ESP-IDF). The pure-marker boards (`mps2-an385`,
      `stm32f4`, `freertos`, `esp32s3`, `fvp-aemv8r-smp`, `s32z270dc2-r52`) still
      RELY on `nros::__register_linked_rmw()` → `nros_rmw_zenoh::register()` (their
      Cargo/src comments say so). C5a's per-board work (board owns the optional
      `nros-rmw-*` dep + a `#[cfg(rmw-X)]` boot-path `register()`) must land FIRST,
      else the drop silently breaks embedded backend registration
      (`Executor::open` → `Transport(ConnectionFailed)`) — and that can only be
      validated on the QEMU/Zephyr/FreeRTOS harness (currently red, #58/#59).
      So the final drop = **C5a-boards (the ~6 linkme-blind boards) → remove
      nros's `rmw-*`/`platform-*` features + the `__FORCE_LINK_{ZENOH,XRCE,
      CYCLONEDDS_SYS}` statics + the `__register_linked_rmw` body + the platform
      mutual-exclusion `compile_error!` + the `platform-posix` cfg force-link from
      `nros/src/lib.rs` → embedded smoke on the harness.** Plus issue #61 (zephyr
      cmake). NOT a consumer sweep anymore.

**Parallel dispatch:** Wave 2a = C5a ‖ C5b (boards vs cli — disjoint). Wave 2b =
C6a ‖ C6b ‖ C6c ‖ C3.2 (disjoint example groups + crates). Wave 2c = C5c (solo,
gated on all of 2b). Each cluster: keep the tree building; `just ci`-scope before
handing back.

---

## C6 — Example node pkgs: strip the feature matrix (#60 T5) — WAVE 3

**Owns:** `examples/**` node/component pkgs (NOT the single-binary application
examples — those legitimately pick a platform).
**Blocked-until:** **C5** (the config selection path must exist).

- [x] Remove the `native/freertos/threadx-linux/nuttx/zephyr/esp32` feature
      matrix + inline `platform-*`/`rmw-*` selections from the ~14 reusable node
      pkgs (`examples/workspaces/{rust,c,cpp,mixed}/src/{talker,listener}_pkg`,
      the embedded `examples/qemu-arm-*/`/`stm32f4/` node pkgs); drop `DEPLOY
      native` from `nano_ros_node_register()` in the C/C++ node CMakeLists.
- [x] Entry pkgs link node pkgs with `default-features = false` only; platform/
      RMW flows from board + `system.toml`. Rebuild the workspace fixtures.
- **Acceptance:** node pkgs carry no `platform-*`/`rmw-*` features/deps; the
  workspace fixtures (`workspace-rust-native`, `workspace-cpp-native`, …) build +
  the existing E2E (`deployed_native_system_e2e`, `cpp_multi_node_entry_typed`,
  multi-host) stay green with selection config-driven.

## C7 — Relocate the Zephyr entry scaffolding out of `nros` (#60 T3 residual) — WAVE 4

**Owns (file ownership — concurrent agents coordinate here before touching these):**
`packages/core/nros/src/lib.rs` (the `zephyr_component_main!` macro + the
`platform::zephyr` module), `packages/core/nros/Cargo.toml` (the `platform-zephyr`
feature), `packages/core/nros-macros/src/main_macro.rs` (the `nros::main!` zephyr
`rust_main` branch + its `::nros::platform::zephyr::wait_for_network` call),
`packages/core/nros-platform/` (helper's new home), `packages/boards/nros-board-zephyr/`,
`examples/zephyr/rust/**` (8 example `src/lib.rs` + `Cargo.toml`), and
`zephyr/CMakeLists.txt` (the `_nros_features` strings). **If you are working on
Zephyr / `examples/zephyr` / the zephyr build concurrently, sync with C7 — it
rewrites the Zephyr entry path.**

**Status.** DONE 2026-06-14 (Method A). `nros` now carries ZERO `platform-*`
features. Cargo-check validated; runtime gated on a green Zephyr build (#58/#59).

**Method chosen — A (entry macro stays in `nros` as framework API; delete the
feature).** Two alternatives were explored and rejected:
- *Fold into `nros::main!`* (one uniform entry macro) — IMPRACTICAL: `zephyr_component_main!`
  is a `macro_rules!` that can't move into the `main!` **proc-macro** crate, and the zephyr
  examples are **lib-only `staticlib`** crates (`rustapp`, no bin) that don't fit `main!`'s
  bin-based Form-1 (which registers via `<pkg>::register` and emits `fn main`). Would need new
  proc-macro work, unvalidatable, for 8 zephyr-only examples.
- *Move macro → `nros-board-zephyr`* (board-owns-bringup) — REJECTED on UX + maintainability:
  it leaks `nros_board_zephyr::` into user source (issue #49-class coupling; native examples
  never name their board in source), fragments the zephyr entry across 3 crates, and forces a
  board crate to dep the full `nros` umbrella.
- **A (chosen):** the real #60 violation is the `platform-zephyr` *feature* (a platform
  selector on `nros`), NOT the entry macro. Entry macros are framework API — `nros::main!`
  already emits the Zephyr `rust_main` from `nros-macros`. So: keep `zephyr_component_main!`
  in `nros`, re-gate it `rmw-cffi`-only, and DELETE the feature. Zero UX change
  (`nros::zephyr_component_main!`), no fragmentation, no proc-macro risk.

**What landed.**
- **Step 1** (`f4197f1f5`): `wait_for_network` → `nros_platform::zephyr::wait_network`
  (C-symbol wrapper; NOT `ZephyrBoard::wait_link_up`, whose `net_if_is_up`/`k_msleep` are
  static-inline headers → native_sim undefined refs); deleted `nros::platform::zephyr`;
  repointed all 3 callers (macro, `nros::main!` branch, codegen).
- **Step 2** (Method A): `zephyr_component_main!` gate `all(rmw-cffi, platform-zephyr)` →
  `rmw-cffi` only (also FIXES its post-C6g availability — examples enable `rmw-cffi`, not the
  dropped `platform-zephyr` forward); **deleted `nros/platform-zephyr`**. The Zephyr platform
  impl + `wait_network` come from `nros-platform[platform-zephyr]` (a dep the examples already
  enable). No example `Cargo.toml`/`src` change needed; the `zephyr/CMakeLists.txt`
  `platform-zephyr` strings target `nros-c`/`nros-cpp` (which keep the feature, D3) — untouched.

**Validation.** Cargo-check: `nros` builds default + agnostic + no_std; no lingering
`platform-zephyr` in the `nros` crate; ARCHITECTURE §2 reframed (entry macros = framework API,
not platform-impl). Runtime (the `rust_main` wiring on `west`/QEMU) is gated on Zephyr going
green (#58/#59/#61).

**Acceptance — MET.** No `platform-*` feature in `nros/Cargo.toml`; no
`#[cfg(feature="platform-*")]` in `nros/src` (the macro is `rmw-cffi`-gated); ARCHITECTURE §2
residual reframed to the entry-macro principle.

## Acceptance (phase)

- [x] No core or user-lib crate (`nros`, `nros-node`, `nros-c`, `nros-cpp`,
      `nros-core`, `nros-params`, `nros-log`, `nros-serdes`, `nros-orchestration`)
      carries `platform-*` or concrete-`rmw-*` features or concrete-backend deps;
      only the vtable interface crates do. (C7 DONE — `nros/platform-zephyr` deleted.)
- [x] No `#[cfg(feature="platform-*")]` in core/user-lib `src/` (C7: the zephyr
      entry macro is now `rmw-cffi`-gated framework API, not a `platform-*` cfg).
- [x] Boards gate concrete RMW optional; selection is board+config-driven.
- [x] Example node pkgs are platform/RMW-agnostic; workspace selection is
      `system.toml`-driven end-to-end.
- [x] RFCs state the agnosticism contract.
- [ ] `just ci` green (not run end-to-end this convergence; validated by scoped
      builds/tests + the embedded runtime smoke — see Status).

> **Archived 2026-07-16.** This doc was left in the active roadmap after the
> 2026-06-15 completion. Every residual it tracked has since closed: zephyr
> smoke unblocked (#58/#59 archived; west/SDK builds green — e.g. the
> phase-287-era FVP cyclone lane links), esp32 live-pubsub fixed (#190,
> archived), the registration-trigger unification landed (phase-249,
> archived), and the `just ci` box is superseded by the phase-287 W7
> full-matrix sweeps. No open work remains.

**PHASE COMPLETE (2026-06-15) — issue #60 closed + archived.** All clusters
landed (C1 boards / C2 nros-node / C3.1+C3.2-via-D3 / C4 docs / C5a-c / C6+tail /
C7); `nros`/`nros-c`/`nros-cpp` are RMW+platform-agnostic at the feature/dep/cfg
layer; selection is board+config-driven (RFC-0031). Runtime-green on
freertos/threadx-rv64/nuttx/baremetal. Residuals (tracked elsewhere, NOT
agnosticism-code): esp32 live-pubsub + zephyr smoke (#58/#59), the full `just ci`
sweep, and the registration-trigger unification (#62 / phase-249).

## Notes

- Keystone risk is C5 (cargo feature unification: whatever turns on
  `nros/platform-X` propagates graph-wide — the model must move that switch to
  the board/build, not a user feature). Land C1–C4 first so C5 has the optional
  boards + vtable ops to build on.
- Single-binary APPLICATION examples (`examples/native/rust/{talker,listener}`,
  `[[bin]]`) may keep an explicit platform — they're apps, not reusable libs
  (see issue #60 + #49).
