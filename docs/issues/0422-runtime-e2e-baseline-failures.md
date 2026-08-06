---
id: 422
title: "10 runtime E2E failures on FRESH fixtures — triage index"
status: open
type: bug
area: testing
related: [issue-0427, issue-0428, issue-0429, phase-336, rfc-0051]
---

## Symptom

`just ci` (tier 1) passes every gate, then fails `test-all`. On 2026-08-05, with
the native lane rebuilt IMMEDIATELY before the run:

```
Summary [118.899s] 1259 tests run: 1242 passed, 17 failed, 72 skipped
Real failures: 10
```

## The number depends on fixture freshness — measure after a rebuild

An earlier run of the same tree reported **19**. Nine of those were STALE
FIXTURES, not defects: upstream's `ad7752bc9` arrived in a rebase and touched
`packages/api/nros/src/node.rs`, so every fixture built before it read stale.
Any pull or rebase does this (CLAUDE.md, "fixture mtime treadmill").

That cost a wrong issue — 0428, filed against CycloneDDS, was the stale-fixture
symptom. **Rebuild the lane, then triage.** A failure list captured across a
rebase is measuring the rebase.

## Diagnosed

| Failures | Cause | Issue |
| --- | --- | --- |
| `nano2nano` gid + sequence | tests grep listener trace output the binary no longer emits; re-verified on fresh fixtures | **0429** |
| ~~cyclone × 5~~ | ~~backend~~ — stale fixtures; 8/12 pass after rebuild | 0428 (filed in error) |
| ~~`cpp_multi_node_entry`~~ | stale SystemModel — a resolver fix never reaches an existing model | **0427** (real; test passes after regenerating) |
| ~~`logging_smoke_mps2_baremetal`~~ | lane coverage, as suspected — name carried no platform token, so `lane-filter.sh` never excluded it | fixed here (renamed) |

### `logging_smoke_mps2_baremetal` — not a defect, a name

`scripts/test/lane-filter.sh` builds its exclusion tokens from the LEADING
CamelCase word of each `PlatformId` variant, so the set is exactly:

    esp32 freertos fvp nuttx px4 qemu threadx zephyr

This fixture's variant is `QemuBaremetal`, i.e. token `qemu` — but the test was
named `mps2_baremetal`. Neither `mps2` nor `baremetal` is a token, and `mps2`
never enters the set at all, because `FreertosMps2` contributes only
`freertos`. Tier 1 selected it and failed on a fixture only
`just qemu build-fixtures` builds.

Renamed to `logging_smoke_qemu_baremetal_mps2_emits_every_severity`.

Second occurrence of this class in one file — `threadx_linux` was the first
(issue 0407). The doc-comment added then even listed this test as tokenised,
counting `mps2` as if it were one. **Check a candidate name against
`bash scripts/test/lane-filter.sh native`; do not eyeball it for
platform-looking words.** A gate that asserts this mechanically is the real
fix and does not exist yet.

## Triaged since (2026-08-06, freshly rebuilt fixtures)

- `native_orchestration_misuse::launch_arm_is_a_removal_error` — **FIXED.** It
  failed BY SUCCEEDING: it asserted `nros::main!(launch = …)` must be rejected
  (phase-296 R-code.1 removed the arm), but phase-330 W7 brought the arm back as
  the SUPPORTED spelling and the test was not retired with it. A test that
  outlives the rule it guards inverts into a guard against the current contract.
  Now `launch_arm_resolves_the_bringup`, asserting the arm compiles.
- `native_orchestration_tiers` ×2 — **issue 0438.** The test greps a
  `multi-tier` marker that only the NUTTX board emits; the native/linux board
  prints the generic `NullNodeRuntime` fallback, so the assertion is
  unsatisfiable on native regardless of router state.
- `zero_copy::test_zero_copy_message_info` — **confirmed 0429's cause**, not
  merely "may be". Verified by running the zero-copy listener directly against a
  router: session opens, subscriber declares, and it emits ZERO matches for both
  strings the test waits on (`"Waiting for"` readiness, then two `"seq="`).
  Upstream's 0429 fix retargeted `nano2nano` at the publisher shim's trace but
  did NOT cover this test — and must not, since its subject is the receive-side
  zero-copy trampoline. Now **issue 0441**, which also records that the fixture
  has no `cfg(feature)` branch at all, so the zero-copy and plain listeners are
  indistinguishable at the output level.
- `large_msg::test_xrce_e2e_integrity` — now PASSES.

## Remaining, untriaged (4)

- `xrce_ros2_interop::test_ros2_action_xrce_client` — accepted=true,
  got_feedback=false
- `realtime_tiers_e2e::realtime_tiers` — 1 of 16 rows
- `native_example_reqresp` — 1 of 18 cells, `cpp/xrce/action`: client never logs
  `Result received:`. Surfaced by the box run below, so it is NOT in the host
  baseline at the top. Third XRCE action-path row here — see that section.
- (the `logging_smoke` line is diagnosed and fixed above)

## Independently reproduced on a second tree (2026-08-06)

Run at `457c8838b`, i.e. BEFORE the "Triaged since" fixes above — so read this
as corroboration of the original 10, not as a current status. The same 10 came
back from a run in the ROS distrobox mirror
(`nano-ros-box`, Ubuntu 22.04) — a different tree, toolchain and glibc from the
host Arch run above. Same list, so none of these is host-specific:

```
suite: tests=1259  failures=10  skipped=23
```

Two deltas against the host run:

- **NEW: `native_example_reqresp` — 1 of 18 cells, `cpp/xrce/action`.** "client
  never logged the server-computed result (`Result received:`)". Not in the host
  list. Shares a shape with the two XRCE rows already here
  (`large_msg::test_xrce_e2e_integrity`,
  `xrce_ros2_interop::test_ros2_action_xrce_client`) — worth triaging as one
  XRCE group rather than three unrelated tests.
- Absent: `realtime_tiers_e2e` and `cpp_multi_node_entry` (0427 — already fixed
  by regenerating the model).

### Compare the junit `failures=`, never nextest's summary line

The box run's raw summary said **33 failed**; the real number was 10. The other
23 were `nros_tests::skip!` panics, which only `just test-all`'s junit rewrite
turns back into skips (CLAUDE.md). The box skips more than the host — no
`rmw_zenoh_cpp` overlay (6 `interop_e2e` cells, 5 `params`, `qos_override_e2e`),
no qemu lanes — so its raw count is inflated by exactly the things it correctly
declined to run.

Raw failure counts are therefore **not comparable across environments** and mean
little on their own. Read `failures=` off
`target/nextest/default/junit.xml`. This sits alongside the freshness note
below: one number is wrong if the fixtures are stale, the other if the
environment differs, and both look like "the tier got worse".

## Method note

Reproduce OUTSIDE the harness, then compare against a working sibling — that is
how 0427 and 0429 were found. But 0428 shows the limit: reproducing a SYMPTOM
outside the harness proves the symptom is real, not that you have its cause. The
binary was stale, and a stale binary fails the same way a broken backend does.

Check freshness the way the harness does (the whole input set), not by
hand-picking one source file to compare against.

## Independent re-measurement, 2026-08-06 (phase-338 `just ci`)

A tier-1 run on freshly rebuilt native fixtures, from a tree carrying phase-338's
completion (the `-entry` collapse, the `log`-facade unification, the C/C++
formatter widening):

    junit: tests=1266  failures=5  skipped=35

The five are all on this index and none is new:

| failure | tracked as |
|---|---|
| `native_orchestration_tiers` ×2 | 0438 |
| `zero_copy::test_zero_copy_message_info` | 0429 / listed here |
| `large_msg::test_xrce_e2e_integrity` | listed here — but see below |
| `entry_e2e::entry_matrix` (60 s TIMEOUT) | not currently listed |

Two notes for whoever owns this index:

* **`large_msg::test_xrce_e2e_integrity` is recorded above as "now PASSES"** and
  it failed here — `Expected 0 invalid messages, got 15`, with 64-byte
  `valid=false` records interleaved among 512-byte `valid=true` ones. Either it
  is flaky or it regressed; the interleaving pattern suggests two publishers'
  payloads on one topic rather than a corruption.
* **`entry_e2e::entry_matrix` is not on the list** and timed out at 60 s in both
  of two consecutive runs here, so it is not a one-off.

Method note, in case the numbers are compared: 77 additional failures appeared
in a first attempt purely because `just format` ran AFTER the fixture build —
every touched example then read STALE. Rebuilding and re-running without
touching sources took it to 5. A count from this suite is only meaningful if
nothing rewrote a source between the build and the run.

