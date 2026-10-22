---
rfc: 0051
title: "Test-matrix architecture: one generated matrix, one output checker, one isolation allocator"
status: Stable
since: 2026-07
last-reviewed: 2026-10-22
implements-tracked-by: [phase-295, phase-510]
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

**Two lists since phase-324 (issue 0352).** `matrix::CELLS` holds baked,
self-contained cells only. Interop and bridge cells carry a shape `Cell` cannot
express — a nano side that is BUILT plus an ephemeral PEER and a DIRECTION — so
they live in `crate::interop::CELLS` as `InteropCell`s (the nano `Cell` +
`build`/`peer`/`dir`/`test`). Both types implement `trait TestCell` so `ci_lane`
selection and coverage iterate one type. The correspondence between what is
tested, built and run is a `Binding` per interop cell, gated by
`tests/matrix_fixture_coverage.rs` G1–G4 — a cell whose declared coordinate
disagrees with what its test builds (the 0341 defect-2 drift) is a gate failure,
not the silent pass the pre-324 `Kind::Interop | Kind::Bridge` fixture-coverage
exemption allowed. Build methods stay separate (no `build_for` unifier; interop
artifacts are NOT moved into `fixtures.toml`).

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

### 6. Two more first-class tables (amendment — phase-329 W0)

`matrix::CELLS` (§1) covers "does the standard node deliver on this
coordinate". Two families the tree grew AROUND it are declared here as
first-class tables so they stop being implicit matrices hand-listed per file:

**6a. Realtime-dim matrix — `matrix::sched_dims::CELLS`.** The scheduler-dim
"applied" family (`*_core_pin_applied` ×5, `zephyr_edf_deadline_applied`,
`threadx_{preempt_threshold,time_slice}_applied`,
`nuttx_{sporadic_budget,tier_priority}_applied` — 10 files, one boot each) is a
`dim × platform × lang` matrix with no table. Declared as:

```rust
pub enum SchedDim {            // the RT knob whose honoring is asserted
    CorePin, EdfDeadline, PreemptThreshold, TimeSlice, SporadicBudget, TierPriority,
}
pub struct SchedCell {
    pub dim: SchedDim,
    pub platform: Platform,    // reuses §1 Platform
    pub lang: Lang,            // reuses §1 Lang
    pub accept_marker: &'static str,   // "<dim> honored" runtime line
    pub fallback_marker: &'static str, // the sanctioned degrade line
    pub tier: Tier,
}
```

The invariant is two-mode (issue 0380): the boot emits EITHER the accept marker
(kernel took the knob) OR the fallback marker (documented degrade) — a SILENT
drop is the failure. One `#[rstest]` over `sched_dims::CELLS` replaces the 10
files; a build-stage gate asserts every dim a model declares has a cell, so a
regeneration that strips a dim (0380) fails structurally instead of silently.
The cells boot the `ws-realtime` fixtures already in `fixtures.toml`.

**6b. Negative-diagnostic registry.** The E1 "no compilation/failure in tests"
rule had one sanctioned exception: a diagnostic whose subject MUST fail to
build/configure was held to be un-prebuildable, so it ran at test time. That set
is an explicit table (`negative_diagnostic_registry.rs`) — each entry names the
tool it invokes (`cmake`/`cargo`/`cc`) and why it cannot be prebuilt — and a gate
greps `tests/` for build-tool invocations and fails on any file NOT in the
registry (the 0196 rule: gate the class, not the known sites). Positive-compile
checks do NOT belong there — they move to the fixture build stage (the phase's
W5).

**Amended (issue 1620): a must-fail compile is not, by itself, a reason to be
on the registry.** The premise conflated a fixture whose artifact is the
compiled output with one whose artifact is the compile's VERDICT. The
`cargo-check-verdict` / `cmake-configure-verdict` builders run the compile in
the build stage, record its exit status and stderr, and succeed whatever it
said; the test asserts the verdict through `require_compile_verdict`.
`linux_main_macro_misuse`, `cmake_platform_matrix`, `cmake_node_register_misuse`
and `diagnostic_verbatim` moved that way (and `native_orchestration_misuse` was
deleted as a duplicate of the `orch_tiers_multi` build). A configure verdict's
dependency closure comes from cmake's own `--trace-format=json-v1` record,
because a FAILED configure writes neither `Makefile.cmake` nor `build.ninja`.
What remains registered states what ELSE keeps it at run time.

**Open question 2 resolved (as implemented):** workspace `Mixed` is a
first-class `Lang` value, NOT a workload — the `Lang` enum in the shipped
`matrix.rs` is `Rust | C | Cpp | Mixed`, and coverage/allocation treat it as a
language coordinate. (OQ2 below marked resolved.)

### 7. The configuration axis, the feature-lane registry, and cell-to-test binding (amendment 2026-10-22 — phase-510)

§1's matrix answers "does the standard node deliver on this coordinate". It
does not answer "does it deliver **built this way**": loan publish, a
multi-session zenoh pool, trigger/wake dispatch, `signal_fd` wake, the
component runtime, a custom transport. Those are cargo features, and today they
live outside every table. A classification of all 195 `nros-tests` targets and
149 per-crate test files (2026-10-22, recorded in phase-510) measured where
that leaves them:

- **6 targets nothing runs.** Four are behind a feature no lane enables:
  `executor_arena_advisory` (missing from `required-features-tests`'s list),
  `wake_latency_cortex_m3` (a file-level `cfg` with no `[[test]]` entry, so it
  builds an empty binary), and `nros-rmw-zenoh`'s `session_config_keys` and
  `status_events_matrix`. `fvp_smoke` is excluded from every runner, and
  `nros-rmw-xrce-cffi`'s `register_smoke` sits in a crate `test-unit` excludes.
- **About a dozen targets reached only by a one-off recipe**, never by a
  tiered lane: `loan_e2e` and `zenoh_integration` (`test-zpico-multisession`),
  `matched_events_live`, `board_import`, `fvp_runtime_ws`, the PX4 SITL tests.
- **No coordinate for any build configuration.** The matrix cannot say
  "lending on FreeRTOS: not covered", because it has no word for lending.
  Issue 0814's history is the cost: the loan path answered differently with and
  without a heap for months, and the posix-only test lane could not see it.
- **48 runtime targets the matrix knows only implicitly**, through the fixture
  resolver's row attribution. No cell names them, so "which test answers this
  cell" is inferred from fixture paths.

Three rules close this.

**7a. `Config` is an axis, distinct from `Workload`.** A *workload* is what the
test makes the image do; it changes what the test observes (a reply, a goal, a
parameter read back). A *configuration* is how the image is built to do it; it
changes the mechanism and must not change the observation. The test that tells
them apart: if switching it off should change what the test asserts, it is a
workload; if the same messages must still arrive by another path, it is a
configuration. Lending is a configuration: a lending pubsub must deliver exactly
what plain pubsub does.

The name follows RFC-0066, which already calls this "a CONFIGURATION is a
fixture axis". It is not `Capability`, which `lane_scope` uses for HOST
capabilities a lane claims (issue 1758), and it is not RFC-0066's "feature",
which is a node package.

```rust
pub enum Config {          // a build-time switch that changes the MECHANISM
    Lending, Multisession, TriggerWake, SignalFdWake,
    ComponentRuntime, CustomTransport, /* … phase-510 W0 decides the set */
}
pub struct Cell {
    // … §1's fields …
    pub config: &'static [Config],   // a SET; empty = the default build
}
```

Three properties follow from the definition and are what make it a separate
axis rather than more `Workload` values:

- **It is a set.** Lending plus multisession is legal, and each modifies a
  workload; a `Workload::LendingPubsub` would repeat Pubsub's assertions and
  could not compose.
- **Its defects are platform-shaped.** A configuration switch fails where the
  build differs — `no_std`, no `alloc`, a different allocator or toolchain —
  so its coverage pairs with `platform` (RFC-0061's selection table), where a
  workload is 1-wise.
- **Its absence is often principled.** "This backend cannot lend" (Cyclone,
  XRCE, uORB, since issue 0814) is a `CarveOut` with a reason, the same E5
  rule §1 applies to platforms — not a cell nobody wrote.

A configured cell is backed by a fixture row whose build enables the
configuration (the row states the cargo features or knobs), and by a test that
asserts the workload's normal observation **plus** what the configuration is
for. Delivery alone is not enough for a configuration whose point is a cost: a
lending cell that only proves delivery passes whether or not the loan avoided
the copy, which is the "green while every byte is still copied" lane issue 0814
already rejected.

**7b. One feature-lane registry.** Every cargo feature that gates a test
target — a `[[test]] required-features` entry or a file-level
`#![cfg(feature = …)]` — is named in ONE table that says which lane enables it
and which targets it runs. `required-features-tests`, the inline feature runs
in `test-all`, and `test-zpico-multisession` are generated from it rather than
spelled by hand. Two kinds of feature are registered, and the registry records
which:

- a **product configuration** (lending, multisession), which also has a
  `Config` value and matrix cells (7a);
- a **test-harness gate** (`trigger-test`, `component-runtime-test`), which
  only decides whether a test file compiles and needs no cell.

The gate: a feature-gated test target that no registered lane enables FAILS.
That is `check-required-features-reachable`'s rule widened from `[[test]]
required-features` to file-level `cfg`s and to every crate, because 4 of the 6
unreached targets above slipped through exactly that gap.

**7c. Every runtime test is bound to a cell, as interop tests already are.**
`interop::assert_test_bound` binds an interop test to its coordinate in both
directions; matrix cells have no such binding. A runtime test target either
binds to `matrix::CELLS` coordinates the same way, or is listed in a table of
non-cell tests with the reason it has none (a static repo check, a docker
edition test — RFC-0051's own E1 shape). A target in neither is a gate failure.
The one-off-recipe-only class becomes visible by construction: such a target
either has a cell, and so a tiered lane that selects it, or a recorded reason.

**What this does not change.** `Kind` stays the wiring axis, `Tier` stays a
property of the cell, and `interop::CELLS` keeps its shape — a configured
interop cell is an `InteropCell` whose nano `Cell` carries a `config` set. The
no-compile-in-tests rule (E1) is untouched: a configuration is a fixture-row
property, built in the build stage.

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

1. ~~Interop axis placement~~ — RESOLVED (phase-324 / issue 0352): interop &
   bridge cells live in `crate::interop::CELLS`, not `matrix::CELLS`, in a shape
   that carries their peer + direction; the reduced workload set (pubsub +
   service, plus lifecycle/qos where a lane exists) is enumerated there.
2. ~~Workspace `Mixed` lang — first-class axis value or a workload?~~ —
   RESOLVED (phase-329 W0): a first-class `Lang` value (`Rust | C | Cpp | Mixed`
   in the shipped `matrix.rs`). See §6.
3. How far to push cyclone/xrce runtime onto QEMU RTOS lanes vs declaring
   CarveOuts (cyclone needs POSIX+CPP; xrce needs an agent locator bake) —
   per-cell decisions live in the phase.
4. Which cargo features are product configurations (a `Config` value with
   cells) and which are test-harness gates (registry only)? §7 states the test;
   the per-feature answer is phase-510 W0's.
