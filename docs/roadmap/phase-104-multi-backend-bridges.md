# Phase 104 — Multi-backend support: cross-domain bridges

**Goal:** allow two RMW backends (e.g., `nros-rmw-uorb` + `nros-rmw-zenoh`)
to coexist in a single nano-ros binary so a bridge node can subscribe
to one transport and republish onto the other. Concrete driver: the
PX4-on-drone bridge that translates uORB topics into Zenoh peer
publishes for a remote ROS 2 stack.

**Status:** Not Started.
**Priority:** Medium. Driven by the PX4-bridge use case identified
during Phase 102 review. Default single-backend builds are
unchanged; multi-backend support is opt-in via a new Cargo feature so
the smallest embedded targets pay nothing.
**Depends on:** Phase 102 (typed entity structs) — `nros_rmw_session_t`
already carries a `void *backend_data` slot per session, which the
multi-instance work extends with a `vtable` pointer to drop the cffi
static singleton.

## Background

Today's nano-ros build picks one RMW backend at compile time. The
mutual-exclusivity rule on `rmw-zenoh` / `rmw-xrce` / `rmw-dds` /
`rmw-uorb` gives every binary a single `ConcreteSession` type, which
the executor wraps. This optimisation is load-bearing for embedded
code-size — linking only one backend's C client trims 60–80 % of
binary footprint vs the upstream "load via dlopen" model.

But it forecloses an important class of deployment: the **cross-domain
bridge**.

### The drone-bridge topology

```
[drone PX4 process]              [companion / cloud]
     uORB topics                       ROS 2 nodes
        ↓                                 ↑
   nros bridge ────── Zenoh ─────── zenohd ─────── rclcpp/rclrs
   (uORB sub +
    Zenoh pub)
```

The bridge subscribes to a small uORB topic set (`vehicle_attitude`,
`sensor_combined`, `vehicle_local_position`, …) and republishes onto
Zenoh keys for the off-vehicle ROS 2 stack. Three reasons this needs
both backends in one binary:

1. **No agent in the middle.** `microxrcedds_agent` exists for the
   XRCE side; nothing equivalent for uORB. A bridge that lives inside
   or alongside PX4 is the cleanest path.
2. **Topic translation is the bridge's job.** PX4 doesn't speak
   Zenoh keys; the bridge maps uORB topic IDs ↔ ROS-2-style topic
   names.
3. **Single-binary deployment.** PX4 modules ship as one binary.
   Running two cooperating processes on flight hardware is a step
   backward.

### What's blocking it today

Three load-bearing singletons:

1. **Cargo feature mutual-exclusion** — `compile_error!` if two
   `rmw-*` features are enabled. Bridge needs both.
2. **`ConcreteSession` type alias** in `nros-node` — collapses
   the executor to one Session type at compile time.
3. **`static VTABLE: AtomicPtr<NrosRmwVtable>`** in `nros-rmw-cffi`
   — one registered C backend per process.

## Design

### What stays the same

- **Trait surface unchanged.** `Rmw + Session + RmwConfig` already
  supports multiple Session instances at the type level. No trait
  additions.
- **Single-backend builds unchanged.** Default Cargo features stay
  mutually exclusive; no code-size regression.
- **Single `open()` call per session.** We do *not* adopt upstream's
  `init_options_init` → `init` two-step. Multi-instance doesn't
  require multi-step init.

### What changes

1. **`multi-backend` Cargo feature on `nros`.** Lifts the
   `compile_error!` mutual-exclusion check. Default off. Opting in
   accepts the code-size cost (each backend's C client linked).

2. **`Executor::open_with_session(session, cfg)` constructor.**
   Bypasses the `ConcreteSession` alias. Bridge code:

   ```rust
   let z_session = ZenohRmw::default().open(&z_cfg)?;
   let u_session = UorbRmw::default().open(&u_cfg)?;

   let z_exec = Executor::<ZenohSession>::open_with_session(z_session, exec_cfg)?;
   let u_exec = Executor::<UorbSession>::open_with_session(u_session, exec_cfg)?;

   loop {
       u_exec.spin_once(10);
       z_exec.spin_once(10);
   }
   ```

   The existing `Executor::open` shorthand stays for the
   single-backend convenience case.

3. **Drop static `VTABLE` in `nros-rmw-cffi`.** Embed
   `vtable: *const NrosRmwVtable` in `nros_rmw_session_t`. The
   typed entity struct already accepts opaque pointer fields, so
   this is a one-pointer addition. `nros_rmw_cffi_register` writes
   the vtable pointer into a runtime-supplied session struct rather
   than mutating a global.

### Backend-identifier validation (optional)

Upstream `rmw.h` carries a `const char *implementation_identifier`
on every entity for cross-backend mismatch detection (passing a
fastdds publisher to a cyclonedds context fails the identifier
check). Worth considering for our multi-backend builds: if a user
accidentally wires a `UorbPublisher` into a `ZenohSession`, what
diagnostic do they get?

Today: type-system error at compile time (the trait's associated
types disagree). Multi-backend doesn't change this — `Executor<S>`
is monomorphised, so cross-backend wiring fails to compile.

The runtime-side identifier is upstream's defence against
plugin-loader-induced confusion (every entity is `rmw_publisher_t *`,
implementation-agnostic). Our typed-with-monomorphisation model
catches the same mistake at compile time. Skip the runtime
identifier — adds a pointer per entity for a use case our type
system already covers.

## Memory + code-size budget

Multi-backend cost on a companion-class target (Jetson Orin /
Raspberry Pi):

| Component | Flash | Heap |
|-----------|-------|------|
| zenoh-pico C client | ~80 KB | ~64 KB |
| uORB rmw (intra-process) | ~5 KB | ~0 |
| nros runtime + executor | ~30 KB | per-arena |
| Bridge logic | trivial | trivial |
| **Total** | **~115 KB Flash, ~64 KB heap** | comfortable |

On a Cortex-M4 with 256 KB Flash + 128 KB SRAM: tight but feasible
(zenoh-pico's TLS feature would stay off). On a Cortex-M0+: not
viable — code size alone breaks the budget.

This validates the opt-in design: default builds unchanged, only
binaries that explicitly opt in pay the cost.

## Work Items

- [ ] **104.1 — `multi-backend` Cargo feature.**
      Add a `multi-backend` feature on `nros` that lifts the
      mutual-exclusion `compile_error!` check on the four `rmw-*`
      features. Default off. Audit the codebase for any other
      assumptions of single-backend (build.rs cfg emissions, type
      aliases, etc.) and feature-gate them appropriately.
      **Files:** `packages/core/nros/Cargo.toml`,
      `packages/core/nros/build.rs`,
      `packages/core/nros-node/Cargo.toml`,
      `packages/core/nros-node/src/session.rs` (the
      `ConcreteSession` alias's `cfg` block).

- [ ] **104.2 — `Executor::open_with_session` constructor.**
      Add `Executor::<S: Session>::open_with_session(session: S,
      config: ExecutorConfig) -> Result<Self, Error>` that takes an
      already-opened Session by value. Existing `Executor::open`
      stays — it constructs the `ConcreteSession` from `RmwConfig`
      and calls the new path. Document the convention: single-backend
      apps use `open()`, multi-backend bridge apps use
      `open_with_session()`.
      **Files:** `packages/core/nros-node/src/executor/mod.rs`,
      `packages/core/nros-node/src/executor/session.rs`.

- [ ] **104.3 — Drop static `VTABLE` in `nros-rmw-cffi`.**
      Embed `vtable: *const NrosRmwVtable` in `nros_rmw_session_t`
      (C side) / `NrosRmwSession` (Rust side). `nros_rmw_cffi_register`
      becomes session-scoped: it writes the supplied vtable pointer
      into the session's `vtable` field rather than a global. Existing
      `register-then-open` flow becomes `open(&vtable, &out_session)`.
      Update the typed-struct roundtrip test from Phase 102.5 to
      drive two simultaneous sessions with two stub vtables.
      **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_entity.h`,
      `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`.

- [ ] **104.4 — Bridge example.**
      `examples/native/rust/bridge/uorb-to-zenoh/`. Subscribes to
      `vehicle_attitude`, `sensor_combined`,
      `vehicle_local_position` via `nros-rmw-uorb`; republishes onto
      Zenoh keys via `nros-rmw-zenoh`. Built against PX4 SITL via the
      Phase 98 fixture. Topic-name translation table embedded as a
      `phf` perfect-hash so adding a new uORB↔Zenoh mapping is one
      table-row change.
      **Files:** `examples/native/rust/bridge/uorb-to-zenoh/`
      (new crate), workspace `Cargo.toml` exclude.

- [ ] **104.5 — Bridge E2E test.**
      `packages/testing/nros-tests/tests/bridge_uorb_to_zenoh.rs`.
      Boots PX4 SITL via the Phase 98 `Px4Sitl::boot_in()` fixture,
      runs the bridge example, runs a host-side rclcpp listener via
      the existing ROS 2 interop fixture, asserts ≥80 % message
      delivery on at least one topic in a 10 s window.
      **Files:** `packages/testing/nros-tests/tests/bridge_uorb_to_zenoh.rs`,
      `.config/nextest.toml` (slow-timeout group).

- [ ] **104.6 — Book chapter.**
      `book/src/user-guide/cross-backend-bridges.md`. Covers the
      bridge topology, the `multi-backend` Cargo feature flag, the
      memory-budget table, and the bridge example walkthrough.
      Cross-link from `book/src/concepts/ros2-comparison.md`
      ("backend selection at compile time" section, where the
      single-backend constraint is described) and from
      `examples/README.md` if present.
      **Files:** `book/src/user-guide/cross-backend-bridges.md`,
      `book/src/SUMMARY.md`,
      `book/src/concepts/ros2-comparison.md`.

## Acceptance Criteria

- [ ] `nros` builds clean with `--features rmw-uorb,rmw-zenoh,multi-backend`
      on POSIX.
- [ ] Default builds (no `multi-backend`) still fail at compile time
      when two `rmw-*` features are enabled — the
      mutual-exclusion check stays on by default.
- [ ] `Executor::<UorbSession>::open_with_session` and
      `Executor::<ZenohSession>::open_with_session` coexist in one
      binary (verified by the bridge example crate).
- [ ] `nros-rmw-cffi` no longer holds a global `VTABLE`. Two
      simultaneous `CffiSession::open` calls with different stub
      vtables both succeed (verified by an extension to
      `tests::typed_struct_roundtrip`).
- [ ] PX4 SITL bridge E2E test green: ≥80 % delivery on
      `vehicle_attitude` over 10 s.
- [ ] Book chapter renders clean (`mdbook build`).
- [ ] No regression in any single-backend test suite (full
      `just test` green).

## Notes

- **Why opt-in instead of always-on?** Code-size: each linked
  backend adds 5–80 KB Flash. Embedded users running a single
  backend don't want to pay for runtime backend-selection plumbing
  they'll never use. Default-off keeps the smallest targets cheap.
- **Why not adopt upstream's `rmw_init_options_t` + `rmw_context_t`
  split?** Our `RmwConfig` + `Session` already covers the same
  ground in fewer steps (one constructor instead of three). The
  three-call dance is upstream working around C's lack of
  constructors; we have Rust + a struct-out-param C calling
  convention, so we don't need it. Multi-instance doesn't require
  multi-step init.
- **Why not adopt `implementation_identifier`?** Upstream's
  cross-backend identifier check defends against
  plugin-loader-induced confusion (every entity is opaque
  `rmw_publisher_t *`, implementation-agnostic). Our typed-with-
  monomorphisation model catches the same mistakes at compile
  time — `Executor<UorbSession>` cannot accept a `ZenohPublisher`
  by type-system construction. The runtime identifier would add a
  pointer per entity for a use case our type system already
  covers.
- **Cross-backend bridges with three+ backends.** Out of scope. If
  someone needs uORB + Zenoh + XRCE in one binary, the same
  pattern extends — three Cargo features under `multi-backend`,
  three Executors. The work in this phase is the *enablement*; the
  combinatorics are the user's problem.
- **Hot-path latency.** The bridge runs two `spin_once` loops back
  to back. Each `spin_once` drives one Session's I/O. For a
  100 Hz uORB topic going to a 100 Hz Zenoh peer, the bridge
  budget is ~5 ms / loop. Acceptable.
- **Memory partitioning.** The two Sessions allocate from the same
  heap (or arena, for `nostd-runtime` backends). On bare-metal
  Cortex-M3 the executor's arena would need to be sized for the
  union of both backends' demands. Documented in 104.6.
