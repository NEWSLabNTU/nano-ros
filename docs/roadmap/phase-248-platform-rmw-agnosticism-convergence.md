# Phase 248 — Platform/RMW-agnosticism convergence

**Goal.** Converge the code onto the target architecture audited in **issue #60**:
core packages + user C/C++/Rust libraries are platform- AND RMW-agnostic (carry
only functional features); RMW + platform reached purely via the vtable ABI;
hardware specifics in board packages; workspace RMW/platform selection
config-file-driven (RFC-0004/0031). This phase closes the convergence debt; it
does NOT redesign — the target is already RFC-0004 (config) + RFC-0031 (RMW
selection) + the platform vtable (`nros-platform-api`/`-cffi`) + the RMW vtable
(`nros-rmw-cffi`).

**Status.** Proposed 2026-06-14. Implements issue #60. **Wave 1 COMPLETE
(2026-06-14): C1 (boards), C2 (nros-node), C3.1 (nros-c/nros-cpp), C4 (docs)
all landed + integration-verified** (umbrella builds zenoh+posix; nros-node
162+5 / nros-rmw 44 / cyclonedds 15 pass; native cross-process pub/sub e2e green
after rebuild — validated C2's runtime wake-probe). Next: C5 keystone (Wave 2).

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

- [ ] **T1 — drop unconditional `nros-rmw-cyclonedds` dep.** Today `nros-node`
      links it because `MessageForRmw` + `cyclonedds_register` reference it
      unconditionally (cyclone's type-descriptor registration leaked into core).
      Make the descriptor-registration a GENERIC vtable hook (the RMW that needs
      per-type descriptors registers via `nros-rmw-cffi`, not a named-backend dep
      on the core executor). Drop the `nros-rmw-cyclonedds` + `-sys` deps from
      `nros-node/Cargo.toml`.
- [ ] **T3 — route platform wake/alloc/spin through the vtable.** Remove the
      `#[cfg(feature="platform-{zephyr,freertos,nuttx,threadx}")]` branches in
      `executor/{node_wake,wake_alloc,spin}.rs`; call the `nros-platform-api`
      `wake_*` / alloc / spin ops generically (the vtable already defines them).
- [ ] Delete the now-unused `platform-*` feature DECLARATIONS from
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

- [ ] Establish the **config/board-driven RMW+platform selection** so the
      `nros` umbrella no longer needs `rmw-*`/`platform-*` features: the board
      crate (selected by entry `[package.metadata.nros.entry] deploy=` /
      `system.toml` `[deploy.<id>]`) brings the concrete RMW + platform backend
      into the link graph; `nros` consumes only the vtable shims. RMW value from
      `system.toml` `[system].rmw` / `[deploy.<id>].rmw` (RFC-0031) drives which
      backend the board/build links.
- [ ] Retire `rmw-{zenoh,xrce,cyclonedds}` + `platform-*` features + the optional
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
- [ ] **C5a — Selection mechanism in boards.** Move the backend force-link
      statics (`__FORCE_LINK_{ZENOH,XRCE,CYCLONEDDS_SYS}` + `__register_linked_rmw`)
      and the concrete-backend deps from `nros` INTO each board crate, gated by
      the board's `rmw-X` feature (C1 already made the boards' `nros-rmw-zenoh`
      optional). A board built with `rmw-zenoh` links + self-registers zenoh; the
      platform impl (`nros-platform-<rtos>`) likewise comes from the board.
      `nros` KEEPS its `rmw-*`/`platform-*` features through this step (additive).
      Owns: `packages/boards/*` + the force-link block in `nros/src/lib.rs` (read
      side only). Verify a board+backend links a working binary.
- [ ] **C5b — Codegen lowers to the board feature + RFC-0031 amendment.**
      `nros codegen entry` / `nros::main!` / `generate` emit the entry's board-dep
      `features = ["rmw-X"]` (from `system.toml` `[system].rmw`) instead of
      `nros = { features = ["rmw-X"] }`. Amend RFC-0031: lowering target is the
      board feature. Owns: `packages/cli` codegen/entry templates + RFC-0031.

**Wave 2b (migration — parallel by consumer group, AFTER 2a):**
- [ ] **C6a — Migrate Rust workspace + native examples** off `nros/rmw-*`/
      `nros/platform-*`; select via board + `system.toml`. (#60 T5)
- [ ] **C6b — Migrate C/C++/mixed workspace examples** (drop `DEPLOY native` +
      CMake rmw/platform pins → board/config). (#60 T5)
- [ ] **C6c — Migrate embedded examples** (qemu-*/stm32f4 node pkgs). (#60 T5)
- [ ] **C3.2 — Retire nros-c/nros-cpp features** (their C/C++ selection now flows
      from board/CMake, not `nros/platform-*`). Owns: nros-c + nros-cpp.

**Wave 2c (cleanup — AFTER every consumer migrated):**
- [ ] **C5c — Drop nros's `rmw-*`/`platform-*` features + concrete-backend deps.**
      Once `git grep 'nros/\(rmw\|platform\)-'` is clean across examples/fixtures,
      remove the features + the optional `nros-rmw-{zenoh,xrce}` /
      `nros-rmw-cyclonedds-sys` deps + the moved force-link block from `nros`. nros
      now consumes only `nros-rmw-cffi` + `nros-platform-cffi` vtables — fully
      agnostic. Owns: `nros/`.

**Parallel dispatch:** Wave 2a = C5a ‖ C5b (boards vs cli — disjoint). Wave 2b =
C6a ‖ C6b ‖ C6c ‖ C3.2 (disjoint example groups + crates). Wave 2c = C5c (solo,
gated on all of 2b). Each cluster: keep the tree building; `just ci`-scope before
handing back.

---

## C6 — Example node pkgs: strip the feature matrix (#60 T5) — WAVE 3

**Owns:** `examples/**` node/component pkgs (NOT the single-binary application
examples — those legitimately pick a platform).
**Blocked-until:** **C5** (the config selection path must exist).

- [ ] Remove the `native/freertos/threadx-linux/nuttx/zephyr/esp32` feature
      matrix + inline `platform-*`/`rmw-*` selections from the ~14 reusable node
      pkgs (`examples/workspaces/{rust,c,cpp,mixed}/src/{talker,listener}_pkg`,
      the embedded `examples/qemu-arm-*/`/`stm32f4/` node pkgs); drop `DEPLOY
      native` from `nano_ros_node_register()` in the C/C++ node CMakeLists.
- [ ] Entry pkgs link node pkgs with `default-features = false` only; platform/
      RMW flows from board + `system.toml`. Rebuild the workspace fixtures.
- **Acceptance:** node pkgs carry no `platform-*`/`rmw-*` features/deps; the
  workspace fixtures (`workspace-rust-native`, `workspace-cpp-native`, …) build +
  the existing E2E (`deployed_native_system_e2e`, `cpp_multi_node_entry_typed`,
  multi-host) stay green with selection config-driven.

## Acceptance (phase)

- [ ] No core or user-lib crate (`nros`, `nros-node`, `nros-c`, `nros-cpp`,
      `nros-core`, `nros-params`, `nros-log`, `nros-serdes`, `nros-orchestration`)
      carries `platform-*` or concrete-`rmw-*` features or concrete-backend deps;
      only the vtable interface crates do.
- [ ] No `#[cfg(feature="platform-*")]` in core/user-lib `src/`.
- [ ] Boards gate concrete RMW optional; selection is board+config-driven.
- [ ] Example node pkgs are platform/RMW-agnostic; workspace selection is
      `system.toml`-driven end-to-end.
- [ ] RFCs state the agnosticism contract.
- [ ] `just ci` green.

## Notes

- Keystone risk is C5 (cargo feature unification: whatever turns on
  `nros/platform-X` propagates graph-wide — the model must move that switch to
  the board/build, not a user feature). Land C1–C4 first so C5 has the optional
  boards + vtable ops to build on.
- Single-binary APPLICATION examples (`examples/native/rust/{talker,listener}`,
  `[[bin]]`) may keep an explicit platform — they're apps, not reusable libs
  (see issue #60 + #49).
