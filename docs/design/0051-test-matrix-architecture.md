---
rfc: 0051
title: "Test-matrix architecture: one generated matrix, one output checker, one isolation allocator"
status: Stable
since: 2026-07
last-reviewed: 2026-07-18
implements-tracked-by: [phase-295]
supersedes: []
superseded-by: null
---

# RFC-0051 — Test-matrix architecture

## Summary

The test system has grown a split personality. `rtos_e2e.rs` proved the right
shape — 33 (platform × language × variant) cells from THREE `#[rstest]`
functions — but around it sit ~55 hand-written per-cell files
(`*_entry_e2e`, `realtime_tiers_*` ×15, `*_workspace_e2e` ×12,
`*_roundtrip_xprocess_e2e` ×8, `multihost*` ×6) that each re-implement the
same spawn → wait-for-ready → count-deliveries checker inline; 22 files
hardcode the `"Received:"` literal that `nros_tests::output` already names;
27 files carry `175xx`-band port literals that must agree with
`fixtures.toml` bakes BY HAND; and port/domain isolation is four independent
inventions (ephemeral binds on native, `base + variant*10 + lang*100` on
migrated QEMU families, `lang_stride = 0` still colliding on baremetal +
esp32, cyclone domains `50 + lang*3 + variant` on zephyr, `61`/`62` on
threadx). Runtime RMW coverage is ~overwhelmingly zenoh (80/82 workspace
rows) while cyclone/xrce runtime lanes exist only as hand-picked cells.

This RFC declares ONE architecture, in the survey-convergent shape the repo
already half-has:

1. **The matrix is data, generated once** — cells (platform × language ×
   RMW × workload) declared in one table; the fixture rows, the test
   parametrization, and the isolation assignments are all DERIVED from it.
2. **One standard-node output checker** — every example node behaves like
   the stock ROS 2 demo (`talker`/`listener`/`add_two_ints`/`fibonacci`),
   so one checker asserts them all; per-file greps die.
3. **One isolation allocator** — every cell gets a deterministic, unique
   (port, ROS domain) pair from a single formula shared by the fixture
   baker and the test runner, so lanes parallelize by construction instead
   of by nextest serialization groups.
4. **Launch through the RTOS framework's runner metadata** — tests keep
   consuming PREBUILT artifacts (the E1 no-compile rule stands), but the
   run command comes from what the framework's build emitted (Zephyr
   `runners.yaml`, ESP-IDF `flasher_args.json`, the board crate's
   `NROS_BOARD_RUNNER`) instead of hand-rolled `qemu-system-*` lines —
   the `west fvp run` (phase-215.D) pattern generalized.
5. **Micro-test budget** — phase-probe one-offs are retired or promoted
   into matrix cells; drift gates stay (cheap, load-bearing).

## Design

### 1. The matrix (SSoT)

One declaration — `packages/testing/nros-tests/src/matrix.rs` (code, so the
test parametrization consumes it natively) with a small serialized mirror
consumed by the fixture generator:

```rust
pub struct Cell {
    pub platform: Platform,      // Native, ZephyrNativeSim, FreertosMps2,
                                 // NuttxArm, NuttxRiscv, ThreadxLinux,
                                 // ThreadxRiscv64, Esp32Qemu, Stm32F4, Fvp
    pub lang: Lang,              // Rust, C, Cpp  (workspaces add Mixed)
    pub rmw: Rmw,                // Zenoh, Cyclonedds, Xrce
    pub workload: Workload,      // Pubsub, Service, Action, Params,
                                 // Lifecycle, Qos, RealtimeTiers, Multihost
    pub kind: Kind,              // Example | Workspace | Interop
    pub tier: Tier,              // Runtime | BuildOnly | CarveOut(&'static str)
}
```

- The existing `rtos_e2e` `Platform`/`Lang` enums grow into this table; the
  three rstest functions become the single consumer pattern for EVERY
  runtime family (entry, workspace, tiers, roundtrip, multihost).
- Unsupported combinations are declared as `CarveOut("nuttx is arm+riscv
  only by design")` IN the table — the audit's E5 "tribal-memory carve-out"
  smell becomes impossible.
- `examples_fixture_coverage.rs` inverts: instead of hand-listing what
  exists, it asserts fixtures.toml ⊇ matrix (every Runtime cell has a
  fixture row) and matrix ⊇ fixtures.toml (no orphan rows) — the fixture
  table stays human-readable TOML but a generator (`nros-tests
  --bin matrix-gen`, build-stage) emits/verifies the derived columns
  (ports, domains, locators) so rows can never drift from the allocator.

### 2. One output checker

Every example node's observable behavior is the stock ROS 2 demo's:

| workload | node contract |
| --- | --- |
| pubsub | talker prints `Publishing: 'Hello World: N'` (or `Published: N` for Int32), listener `I heard:`/`Received:` with MONOTONIC payloads |
| service | server ready-marker + request marker; client prints the sum |
| action | Fibonacci order-10: goal accepted → feedback prefixes → final sequence |
| params/lifecycle/qos/tiers | the existing `output.rs` marker sets |

`nros_tests::checker::StandardChecker` (one module beside `output.rs`)
takes `(Workload, role, min_events)` and wraps the whole
wait-for-ready → collect → count → assert-monotonic dance that ~55 files
currently re-implement. Literal markers OUTSIDE `output.rs` become a lint
(`non_goals_grep`-style gate: `grep -rn '"Received:"' tests/` must only hit
`output.rs`). ROS-interop lanes reuse the SAME checker against
`ros2 topic echo` / `demo_nodes_cpp` output — the contract is the point:
nano-ros nodes are behaviorally interchangeable with the ROS 2 demos.

### 3. One isolation allocator

`nros_tests::alloc` — a pure function, no state:

```
port(cell)   = 7000 + platform.index()*400 + workload.index()*40
             + lang.index()*10 + rmw.index()          // tcp locator / agent
domain(cell) = 1 + (platform.index()*24 + workload.index()*3
             + lang.index()) % 232                     // DDS domain
```

- Deterministic + collision-free across the whole matrix by construction
  (index ranges are bounded; the generator ASSERTS injectivity at build
  stage — no runtime cost).
- The fixture generator bakes `port(cell)`/`domain(cell)` into
  `NROS_ENTRY_LOCATOR` / `NROS_DOMAIN_ID` rows; the test side calls the
  same function — the 27-file hand-agreement band dies.
- Native lanes KEEP ephemeral allocation (`start_unique`,
  `unique_ros_domain_id`) — already parallel-safe; the allocator is for
  bake-time platforms only.
- Migrates the two `lang_stride = 0` stragglers (baremetal, esp32) onto
  unique ports, retiring `qemu-baremetal-shared`/`qemu-esp32` serialization
  groups; nextest groups remain ONLY for genuinely exclusive resources
  (FVP license, ros2 daemon, QEMU host-load throttling).

### 4. Launch = framework-runner metadata, artifacts prebuilt

The E1 rule stands: tests never build. But the RUN command should be the
framework's, not ours:

- **Zephyr**: read `build-*/zephyr/runners.yaml` (emitted at build stage)
  and construct the emulator invocation from it — or `west build -t run`
  guarded by the existing pristine-stamp so it provably no-ops the build
  half. The `west fvp run` extension (phase-215.D, reads `NROS_BOARD_RUNNER`
  from CMakeCache) is the template.
- **ESP-IDF**: derive from `flasher_args.json` instead of a hand-built
  Espressif-fork command line.
- **NuttX / FreeRTOS / bare-metal** (no framework runner exists): the
  hand-rolled `qemu.rs` builders are ALREADY the convention — keep, but
  move per-machine argument blocks into the board crates'
  `NROS_BOARD_RUNNER`-adjacent metadata so a board owns its launch line
  (phase-215 duty rule), and `qemu.rs` becomes a thin interpreter.
- Where the framework runner is unusable at test time (it would rebuild),
  the bypass is documented AT THE CALLSITE (the E1 sanctioned-exception
  pattern from #222).

### 5. Micro-test budget

- Phase-probe one-offs (`w1d_native_tier_generation_probe`,
  `w1_zephyr_tx_throughput_measure`, `phase_118_collapse`,
  `native_entry_poc_boot`, CLI `phase_212_f_bringup` — E3 violations by
  name alone) are retired or renamed+promoted into matrix cells.
- The ~55 per-cell files consolidate into the matrix consumers; target:
  tests/ file count drops by ~40 while CELL coverage goes UP (the matrix
  makes missing cells visible instead of invisible).
- Drift/shape gates (`example_shape`, `*_drift`, `loc_budgets`,
  `weak_symbol_audit`) are explicitly NOT micro-tests — they stay.
- In-crate unit tests (~660) are out of scope: they test code, not cells.

## Coverage debt the matrix makes visible (initial tier assignments)

- cyclonedds/xrce RUNTIME cells on RTOS platforms — today interop/bridge
  only; the matrix forces an explicit Runtime-vs-CarveOut decision per cell.
- stm32f4 = BuildOnly (hardware-gated, #221 resolution), esp32 C/C++ =
  BuildOnly today, threadx-riscv64 action = missing, embassy = no e2e lane.
- Workspace rows: 62/82 native — RTOS workspace cells get the same
  tier triage.

## Alternatives considered

- **Pure-TOML matrix with a proc-macro expanding tests** — rejected: rstest
  over a `const` table is simpler, debuggable, and already proven in-tree.
- **Ephemeral ports everywhere** — rejected for baked platforms: embedded
  images take the locator at COMPILE time (Kconfig/cmake defs); bake-time
  determinism is the only option, so make it collision-free by formula.
- **`west build -t run` unconditionally** — rejected: triggers the build
  half (violates E1, and a stale-reconfigure can mask museum binaries);
  runner-metadata interpretation keeps the artifact/launch split honest.
- **Deleting the per-family QEMU smoke files outright** — rejected;
  fold them into matrix cells first, delete after the cell is green.

## Cross-refs

- RFC-0026 (example layout — the nodes whose behavior the checker pins),
  RFC-0048 (workspace shape the workspace lanes build), phase-215
  (board-owned runner metadata), phase-289 (rtos_e2e as the proven consumer
  shape), issues #215/#222 (freshness discipline the fixture generator
  inherits), audit checklist §E (E5 coverage matrix; E6–E9 added by this
  RFC's phase).

## Open questions

1. Interop axis placement: `Kind::Interop` cells pair a nano-ros node with
   a real ROS 2 peer per RMW (zenoh/cyclone/fastrtps/xrce) — one workload
   set or a reduced one (pubsub+service only)?
2. Workspace `Mixed` lang — first-class axis value or a workload?
3. How far to push cyclone/xrce runtime onto QEMU RTOS lanes vs declaring
   CarveOuts (cyclone needs POSIX+CPP; xrce needs an agent locator bake) —
   per-cell decisions live in the phase.
