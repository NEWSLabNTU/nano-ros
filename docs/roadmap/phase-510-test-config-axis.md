# Phase 510 — a configuration axis for the test matrix, one feature-lane registry, and every test bound to a cell

**Status (2026-10-22). W0–W4 done; W5–W6 open.** Design is RFC-0051 §7
(amendment 2026-10-22) and RFC-0061's amendment of the same date. This phase
opened from phase-441 W8: the 0814 decision (option 2) keeps the loan publish
half, which by W8's own condition calls for an embedded lending lane. Asking
where that lane would go showed that the test system has no place for it. The
inventory below is what that question measured.

## Why

The test system is structured where it has tables and unstructured everywhere
else:

- **`matrix::CELLS`** (235 cells: 203 Runtime, 16 BuildOnly, 16 CarveOut) is
  cross-checked against `examples/fixtures.toml`, so a Runtime cell nothing
  builds is caught. But a cell names no test.
- **`interop::CELLS`** (42 rows) binds each row to its test binary both ways
  (`assert_test_bound`, `CASE_CELLS`) and records live verdicts. It is the
  one table where cell and test are bound in both directions.
- **Feature lanes**: one real one, `required-features-tests`
  (`just/check/lanes.just:3251-3290`). Its target list is spelled by hand.
- **Build configurations** (lending, multisession, wake mechanisms, the
  component runtime, custom transport) are cargo features with no coordinate
  at all.

So "lending on FreeRTOS" cannot be expressed as covered or not covered, and
issue 0814 shows what that cost. The loan path answered differently with and
without a heap for months. The one lane that ran it was posix with `alloc`, and
no table could say what it was missing.

## Inventory (measured 2026-10-22)

The method was a read-only classification of every test target:

- `git ls-files` gives the targets.
- `git grep` finds each runner: `--test X`, `binary(=X)` and `_nextest-platform X` across `just/`, `justfile`, `.github` and `scripts`.
- Each test file was scanned, comment lines stripped, for the following:
  - a real `assert_test_bound(` call
  - `require_*_in_lane(` or `lane_skip!`
  - resolver calls from `src/fixtures/{binaries,groups,require,lane}`
  - `required-features` or a file-level `#![cfg(feature …)]`
- `.config/lane-admission/gate.txt` membership.

Three rstest-injected files were classified by hand. The "reached by nothing"
rows were re-checked by a second `git grep`.

### `nros-tests` — 195 targets

| How it is managed | Targets |
| --- | --- |
| Bound to an interop cell (`assert_test_bound`) | 20 |
| Matrix-scoped explicitly (`require_*_in_lane`, `PlatformId::…`) | 46 |
| Matrix-scoped only IMPLICITLY — the resolver chokepoints call `fixtures::lane::require_in_lane` (`src/fixtures/binaries/mod.rs:561,574,611,2373`), so the test is coordinate-scoped without naming a coordinate, and no cell knows it | 48 |
| Compile / cmake / west fixtures keyed by fixture ID, not coordinate | 21 |
| Static repo checks (no runtime fixture) | 39 |
| Feature lanes | 18 |
| Docker `ros_editions_*`, no cell | 3 |

### Per-crate `tests/` — 149 files

- 71 in the CLI sub-workspace, run by `check::cli-tests`.
- The ungated root-member files run under `test-unit`.
- The feature-gated ones each run in a named line of `just/check/lanes.just`, except the outliers below.

### Shell tests — `tests/*.sh`, 35

- 31 run from `check::*` recipes.
- 3 run from platform recipes.
- 3 are libraries or entrypoints, not tests.

### Outliers

**Reached by nothing (6).**

| Target | Why |
| --- | --- |
| `nros-tests` `executor_arena_advisory` | `required-features = ["component-runtime-test"]` (`nros-tests/Cargo.toml:456`), but missing from the `required-features-tests` target list (`lanes.just:3253-3257`) |
| `nros-tests` `wake_latency_cortex_m3` | `#![cfg(feature = "trigger-test")]` (line 36) and no `[[test]]` entry. No recipe enables the feature for it, so it builds an empty binary. `freertos.just:254-260` builds its fixture for nothing. |
| `nros-tests` `fvp_smoke` | Excluded from `test-all` (`justfile:2779`). Its test group `zephyr-fvp` is excluded from `test` / `test-integration` (`justfile:1392,1819`). No recipe names it. |
| `nros-rmw-zenoh` `session_config_keys`, `status_events_matrix` | `#![cfg(feature = "platform-posix")]`; no lane enables it for the crate |
| `nros-rmw-xrce-cffi` `register_smoke` | `test-unit` excludes the crate (`justfile:1194`); no recipe names it |

**Reached only by a one-off recipe, never by a tiered lane.**

| Target | Only runner |
| --- | --- |
| `loan_e2e`, `nros-rmw-zenoh` `zenoh_integration` | `test-zpico-multisession` (`justfile:1717,1730`) |
| `nros-rmw-zenoh` `matched_events_live` | `linux test-matched-events` (`linux.just:1006`) |
| `board_import` | Unscoped `just test` / `test-integration` only; `test-all` excludes it (`justfile:2779`) |
| `fvp_runtime_ws` | `verify-fvp-runtime` (`just/zephyr-setup.just:453,481`) |
| `esp32_emulator` | the `test` recipe in `just/esp32.just`, a module the justfile no longer loads; the board is dormant (issue 1525) |
| `ros_editions_*` (5) | `just ros_editions ci` — the edition axis, CLAUDE.md "not a cell" |
| `nros-px4-sitl-test` (4) | `px4 test-*` (`px4.just:454-582`) |

**Feature runs outside any feature lane.**
- `custom_transport_loopback`, `xrce_guard_wake` and `xrce_concurrent_entities` run as inline feature invocations inside `test-all` (`justfile:2732,2741`).
- Cyclone `bare_metal_link` runs only under `test-ignored`.

**Configurations with no coordinate.**
- lending / loan
- multisession
- trigger / wake latency
- `signal_fd` wake
- component runtime / dispatch substrate
- custom transport
- XRCE guard wake

## Work items

### W0 — vocabulary: which features are configurations

Classify every cargo feature that gates a test, applying RFC-0051 §7a's test:
does switching it off change what the test asserts? Each one becomes one of:

- a **product configuration**: it gets a `Config` value and matrix cells;
- a **test-harness gate**: registry entry only, no cell.

The expected split, to be confirmed by reading each target:

| Feature | Expected class |
| --- | --- |
| `lending` / `loan-e2e` | Configuration (`Lending`) |
| multisession pool | Configuration (`Multisession`) |
| `trigger-test` | Harness gate over a configuration (`TriggerWake`) |
| `signal-fd-wake-test` | Configuration (`SignalFdWake`) |
| `component-runtime-test` | Configuration (`ComponentRuntime`) |
| `phase216-substrate` | To decide |
| `rmw` / `link-custom` | Configuration (`CustomTransport`) |
| `xrce-wake-test` | To decide |
| `rclrs-port-test` | Not a configuration: the rclrs talker port is a `Workload::Port` question |
| `platform-posix`, `posix-c-port`, `c-stub-test`, `alloc` | Harness gates |

**Acceptance.** A table in this doc giving every gating feature in the tree, its class, and the reason. A `Config` enum in `matrix.rs` with exactly the configuration rows. Every existing cell gets `config: &[]`, so nothing moves yet.

**DONE 2026-10-22.** Each of the 20 gating features was read against what it
turns on. The table with reasons is `.config/feature-lanes.toml`'s
`[feature.*]` rows, the one place it is kept; the decisions:

| Class | Features |
| --- | --- |
| config `Lending` | `lending`, `loan-e2e` (turns on `nros-node/rmw-lending`) |
| config `SignalFdWake` | `signal-fd-wake-test` (turns on `nros-node/signal-fd-wake`) |
| config `CustomTransport` | `link-custom` |
| config `Multisession` | not a cargo feature: the `NROS_ZENOH_MAX_SESSIONS` knob (`[knob.*]`) |
| harness | `trigger-test`, `component-runtime-test`, `phase216-substrate`, `xrce-wake-test`, `rmw`, `rclrs-port-test`: each pulls test-only dependencies into `nros-tests`; the product code it reaches is always built |
| harness | `platform-posix`, `posix-c-port`, `c-stub-test`, `unix-mock`, `bridge-stub`, `std`, `alloc`, `stats`, `log-compat`, `rmw-cffi`: each selects a platform or a stub so a test can compile |

Two predictions above were wrong: `trigger-test` and `component-runtime-test`
are harness gates, not configurations — they pull dependencies; the trigger API
and the component runtime are in the default posix build. `matrix::Config`
holds exactly the four configuration values, every cell carries `config: &[]`
(`Cell::with` builds a configured one), and the gate holds the enum and the
registry to one set in both directions.

### W1 — the feature-lane registry, and the gate

One table (proposed: `packages/testing/nros-tests/src/feature_lanes.rs`, or a TOML under `.config/` if a shell consumer needs it). Each row: feature set → lane → targets → class (W0).

Generated from it, rather than spelled by hand:
- `required-features-tests`'s invocations
- the inline feature runs in `test-all`
- `test-zpico-multisession`

Widen `check-required-features-reachable` so that a feature-gated target nobody enables FAILS. It must cover:
- every crate, not only `nros-tests`;
- file-level `#![cfg(feature = …)]`, not only `[[test]] required-features`.

Four of the six unreached targets passed through exactly that gap.

**Acceptance.**
- **Negative control:** the widened gate fails on today's tree, naming `executor_arena_advisory`, `wake_latency_cortex_m3`, `session_config_keys` and `status_events_matrix`.
- **Positive:** it passes after W2.
- `required-features-tests`, `test-all`'s feature runs and `test-zpico-multisession` run the same targets as before, with the old recipe lines deleted.

**DONE 2026-10-22.** `.config/feature-lanes.toml` is the registry (TOML: the
recipes are shell). `scripts/feature-lanes.py run <runner>` executes a recipe's
lanes, so `required-features-tests`, `test-zpico-multisession` and `test-all`'s
three feature runs are each one call, and each lane's comment moved beside its
row. `just check required-features-reachable` now runs `feature-lanes.py
check`, and the feature-level `check-required-features-reachable.py` is
deleted. The new gate works per (crate, target):

- every `[[test]] required-features` and file-level `#![cfg(feature …)]` target
  in every crate must sit in a lane whose features satisfy its whole gate (a
  conjunction is a conjunction);
- a `test-unit` lane's claim that the workspace unifies a feature on is checked
  against `cargo metadata` (NOT VERIFIED, never a failure, where the cache is
  absent);
- a hand-spelled runner (`node-std-tests`, `test-ignored`) must carry its
  lane's crate and features in its own text;
- every gating feature is classified, and `matrix::Config` matches the
  config rows.

Negative control, run on the tree before W2: the gate named
`executor_arena_advisory`, `wake_latency_cortex_m3`, `session_config_keys`,
`status_events_matrix` and `matched_events_live` (the last ran only in a debug
recipe). Its self-test holds the shapes: a feature enabled for one target but
not the next, a missing conjunct, an unclassified feature, a generated runner
that never calls the generator, a unified claim the graph does not back.

Measured equivalence: all 16 `required-features-tests` lanes pass (32 tests in
the nros-tests tolerant lane, now including `executor_arena_advisory`), and
`test-zpico-multisession` reports the same 2 passed / 22 filtered and 2 passed
it did before.

### W2 — the six unreached targets

Each one is fixed or deleted, with the reason recorded:

- `executor_arena_advisory`: join its feature lane.
- `wake_latency_cortex_m3`: give it a `[[test]]` entry and its lane. Its fixture is already built.
- `session_config_keys`, `status_events_matrix`: join a lane with `platform-posix`.
- `register_smoke`: a lane, or delete it if `test-unit`'s exclusion of the crate is deliberate.
- `fvp_smoke`: a lane, or a recorded `CarveOut` (FVP needs a licensed model) and deletion of the dead filter.

**Acceptance.** Each target runs green in a lane, or is gone. Run each solo before believing a red (CLAUDE.md: a target nothing ran has not been green for an unknown time).

**DONE 2026-10-22.**

| Target | Outcome |
| --- | --- |
| `executor_arena_advisory` | Joined the nros-tests tolerant lane; passes |
| `session_config_keys` | Joined the zenoh `lending,platform-posix` lane (opens no session); passes |
| `status_events_matrix` | New tolerant zenoh lane (starts a router); passes |
| `matched_events_live` | Same lane, out of its debug-recipe-only state; passes |
| `register_smoke` | New lane, no features — `test-unit` excludes its crate for a platform-gating reason that does not reach this test; passes |
| `fvp_smoke` | Runs in `verify-fvp-runtime`, after `build-fvp-board-import`, beside `fvp_runtime_ws` on the same model |
| `wake_latency_cortex_m3` | **Deleted.** Measured: 0 samples on QEMU — the MPS2 model does not emulate the DWT cycle counter its histogram reads — so it was a red no lane could clear, and there is no hardware lane. The two bench images stay build-checked by their fixture row, and `wake_probe`'s CSV/percentile code keeps its unit tests in `nros-node` |

Correction to the inventory: `wake_latency_cortex_m3` was not "no `[[test]]`
entry, so nothing builds it". `nros-tests` uses auto-discovered tests, so it was
a target — compiled EMPTY by every run without `trigger-test`, which is the
0779 shape.

### W3 — bind runtime tests to cells

Give matrix cells the binding interop cells already have: a `matrix::assert_test_bound`, or a `test` field on the cell, whichever the 46 + 48 targets fit with less churn.

Every runtime target either:
- binds its coordinates; or
- is listed in a non-cell table with its reason (static repo check, docker edition test, fixture-ID compile check).

Gate: a runtime target in neither fails.

**Acceptance.** The gate passes with all 94 matrix-scoped targets bound. Its negative control is a new unbound runtime test file, which it refuses.

**DONE 2026-10-22.** A read of every Runtime cell against the tests that
resolve its fixtures assigned each one its primary test. phase-329 W1's
five-file `W1Consumer` grew into `matrix::Consumer`: 24 values, one per primary
binary, with `consumer_of(cell)` the one assignment, by rule.

- `every_runtime_cell_has_a_consumer` (lib) holds `consumer_of` total over
  Runtime cells, and every consumer owning at least one.
- The five W1 consumers derive their cases from `consumer_of`; multihost and
  roundtrip used to spell their own predicate, and now do not. The other 18
  carry a fixture-free `cases_bound_to_matrix_cells` that calls
  `matrix::assert_consumer_cases` with their case list, so a cell added,
  retiered or reassigned without its consumer's list moving is a red.
- `matrix::NON_CELL_TESTS` gives each of the 150 other binaries its reason:
  `Static` 51, `CompileCheck` 30, `Edition` 5, `FeatureLane` 12, `Secondary`
  29 (each names its primary and what it adds) and `Unmodeled` 23 (each names
  the coordinate or workload the matrix does not model — candidate cells, not
  exemptions).
- `every_test_binary_is_a_consumer_an_interop_test_or_classified`
  (`matrix_fixture_coverage`, already in the gate lane) holds the partition of
  `tests/*.rs` and each consumer's binding. Negative control: a new
  `tests/zz_unbound_probe.rs` was refused, named.

What binding found, beyond the 48:

- **Eleven Runtime cells nothing delivered through.** Nine native workspace
  `EntryPubsub` cells (C, C++, Mixed on zenoh; all three languages on Cyclone
  and XRCE) and ThreadX-Linux Rust `EntryPubsub` were booted only by
  `rmw_coordinate_truth`, which checks node registration, not delivery. They
  are `BuildOnly(NO_DELIVERY_TEST)` now, with what unlocks them.
- **`entry_e2e` ran the wrong image for one.** Its exec table is keyed without
  the RMW, so the Zephyr C++ Cyclone entry cell booted the zenoh image, and
  `workspace-zephyr-cpp-cyclonedds` was resolved by no test. `consumer_of`
  assigns `entry_e2e` zenoh entries only; that cell is
  `BuildOnly(ENTRY_E2E_IS_ZENOH_ONLY)`.
- `ci_lane`'s documented table moved with it (nightly 36 -> 35 cells, same 34
  coordinates), and its tier-3 cell figure, ungated, had read 206 for some time
  where the matrix has 229 Runtime cells.

`assert_test_bound` (interop) keys on four axes; the matrix binding keys on all
five plus the configuration set, because Example and Workspace cells share
(platform, lang, rmw, workload).

### W4 — retire the one-off-recipe-only class

Each one-off-only target above either:
- gets a cell, and so a tiered lane that selects it; or
- moves into the W3 non-cell table with its reason.

`loan_e2e` and `zenoh_integration` get `Config::Multisession` (+ `Lending`) Linux cells. `ros_editions_*` stay off the matrix by the standing edition-axis ruling and are recorded so. `esp32_emulator` becomes a `CarveOut` naming issue 1525.

**Acceptance.** No runtime target is reached only by a recipe outside the tier ladder, unless the W3 table says why.

**DONE 2026-10-22.** Each one-off-only target, now:

| Target | Where it runs, or why it has no tiered lane |
| --- | --- |
| `loan_e2e` | The configured cell `(Linux, Rust, Zenoh, Pubsub, Example) + [Lending, Multisession]`, consumer `Consumer::LoanE2e`, in `test-zpico-multisession` — a dependency of `test-all`, so on the tier ladder |
| `nros-rmw-zenoh` `zenoh_integration` | The same multisession lane |
| `nros-rmw-zenoh` `matched_events_live` | `required-features-tests` (W2), inside `check::build` |
| `board_import` | `NonCell::CompileCheck` — a west fixture, not a runtime test |
| `fvp_runtime_ws`, `fvp_smoke` | `NonCell::Unmodeled`: no FVP cell; both run in `verify-fvp-runtime` |
| `esp32_emulator` | `NonCell::Unmodeled`: ESP32 is dormant (issue 1525) |
| `ros_editions_*` | `NonCell::Edition`, the standing edition-axis ruling |
| `nros-px4-sitl-test` (4) | Not a `nros-tests` binary: a separate crate whose tests need a PX4 SITL tree, run by `px4 test-*` and nothing else |

A configured cell is answered by the test of its configuration, never by its
default-build sibling's consumer (`consumer_of` checks `config` first), and the
cell key carries the configuration set, so `cells_unique` and the bindings tell
the two apart.

### W5 — the first embedded configuration cell: lending on FreeRTOS

The case this phase opened for, and the remaining open half of issue 0814: the zenoh loan is compiled for embedded targets (`just check no-std`) and has only ever run on posix.

- One `(FreertosMps2, Rust, Zenoh, Pubsub, Example, Runtime)` cell with `config = [Lending]`.
- A fixture row building the leaf with `lending`.
- A QEMU test asserting delivery through `try_loan` + `commit`, **and** the copy count on target.

The copy count is required because a lane that proves only delivery passes whether or not the loan avoided the copy. 0814 rejected exactly that lane.

How to measure the copy count on target is part of the work: the W8 host method interposed `memcpy` through `LD_PRELOAD`, which a bare-metal image cannot do. Candidates are a linker `--wrap=memcpy` counter in the test image, or a symbol-level check that the payload path reaches no copy.

Cyclone, XRCE and uORB lending cells are `CarveOut`s naming 0814 option 2.

**Acceptance.**
- The cell is green in tier 3 and selected by the 2n cover.
- The test measures zero payload copies through the loan and one through `publish_raw` on the same image.
- 0814's title gap is closed, or narrowed with the measurement recorded there.

### W6 — the tier covers adopt the axis

Implement RFC-0061's 2026-10-22 amendment in `ci_lane.rs`:
- `config` is 1-wise in tiers 1 and 2;
- pairwise with `platform` in 2n.

Measure the 2n coordinate growth before turning it on, as §Cost is coordinates requires of every change to the cover.

**Acceptance.**
- `lanes_touch_every_declared_value_of_every_axis_they_cover` covers `config`.
- The 2n coordinate delta is measured and recorded in RFC-0061.

## Non-goals

- Re-homing interop cells, or changing `Kind` / `Tier`.
- Moving the edition axis (`ros_editions_*`) into the matrix.
- Pairing `config` with `workload`. RFC-0061's amendment argues it from the defect catalogue.

## Related

- RFC-0051 §7, RFC-0061 amendment 2026-10-22, RFC-0066 (where "configuration" comes from).
- [phase-441](phase-441-rmw-on-target-verification.md) W8, which opened this.
- Issue [0814](../issues/0814-lending-never-exercised-on-hardware.md) (W5).
- Issue 1758 (the no-skip rule every new lane keeps).
