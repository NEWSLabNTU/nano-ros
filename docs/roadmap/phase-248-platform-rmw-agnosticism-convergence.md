# Phase 248 — Platform/RMW-agnosticism convergence

**Goal.** Converge the code onto the target architecture audited in **issue #60**:
core packages + user C/C++/Rust libraries are platform- AND RMW-agnostic (carry
only functional features); RMW + platform reached purely via the vtable ABI;
hardware specifics in board packages; workspace RMW/platform selection
config-file-driven (RFC-0004/0031). This phase closes the convergence debt; it
does NOT redesign — the target is already RFC-0004 (config) + RFC-0031 (RMW
selection) + the platform vtable (`nros-platform-api`/`-cffi`) + the RMW vtable
(`nros-rmw-cffi`).

**Status.** Proposed 2026-06-14. Implements issue #60.

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

- [ ] Each of the 4 boards deps `nros-rmw-zenoh` UNCONDITIONALLY. Make it
      optional behind an `rmw-zenoh` feature (mirror the sibling pattern in
      `nros-board-mps2-an385` / `-stm32f4` / `-nuttx` / `-esp32-qemu`, where the
      backend is optional + the board can build DDS-/XRCE-only).
- [ ] Default-on `rmw-zenoh` is acceptable IF a board's existing examples assume
      it — but the feature must be droppable. Verify each board still builds with
      and without the feature.
- **Acceptance:** `cargo build -p <board>` (default) + `--no-default-features`
  (or `--features` minus rmw-zenoh) both succeed for all 4; no unconditional
  concrete-RMW dep remains. Grep clean: `git grep -L 'optional = true' …` shows
  no unconditional `nros-rmw-zenoh` in these 4.

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
- **Acceptance:** `cargo test -p nros-node` green; `git grep 'feature = "platform-'
  packages/core/nros-node/src` empty; no `nros-rmw-cyclonedds`/`platform-*` in
  `nros-node/Cargo.toml`. Cyclone E2E (`cyclonedds_ros2_interop`) still passes via
  the generic hook.

## C3 — nros-c / nros-cpp: platform decoupling + feature retirement (#60 T2/T3 C/C++)

**Owns:** `packages/core/nros-c/` + `packages/core/nros-cpp/` (all).
**Blocked-until:** phase-1 none (Wave 1); phase-2 after **C5**.

- [ ] **Phase 1 (Wave 1) — platform impls behind the vtable.** Remove the
      `#[cfg(feature="platform-{freertos,zephyr,threadx}")]` `#[global_allocator]`
      + critical-section blocks from `nros-c/src/lib.rs`; route alloc/critical-
      section through the platform vtable (`nros_platform_*` FFI). Same audit pass
      on `nros-cpp/src`.
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
