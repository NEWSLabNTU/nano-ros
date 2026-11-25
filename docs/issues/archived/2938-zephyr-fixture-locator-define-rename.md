---
id: 2938
title: "Renaming the Zephyr locator define moved no leaf signature, so persistent build dirs re-configured without it and every native_sim cell dialled the wrong router"
status: resolved
type: bug
severity: high
area: [zephyr, fixtures, build]
related: [phase-581, 2906]
found: 2026-11-24
resolved: 2026-11-26
---

## What happened

Tier 2 of the nros12 release run-matrix (run 45351389552, main `0d9fa3e470`):
every Zephyr native_sim cell that dials an allocated router failed to open a
session.

- Failing cells: `entry_matrix` (4 of 6), `realtime_sched_tasks`,
  `sched_dims_applied`, `zephyr_ctf_markers` and `multihost` (its Zephyr
  robot1 talker).
- C and C++ guests booted with `locator tcp/127.0.0.1:7447 (baked)`.
- Rust guests baked the `tcp/10.0.2.2:74xx` their bringup states, which is
  unreachable under NSOS.

## Cause

1. phase-581 W6 (`a3f582f9e5`) renamed the define
   `scripts/build/zephyr-fixture-leaves.sh` hands west, from
   `-DCONFIG_NROS_ZENOH_LOCATOR` to `-DCONFIG_NROS_LOCATOR`.
2. The leaf signature (`.nros-zephyr-fixture.sig`) recorded only the locator
   VALUE (`zenoh_locator=tcp/127.0.0.1:<port>`). The value did not move, so no
   persistent build dir was sent back through west.
3. The next re-configure ran on the ninja path from `CMakeCache.txt`, which held
   the old spelling only, as Zephyr's `CLI_CONFIG_NROS_ZENOH_LOCATOR`. Until
   issue 2906 that retired entry refused the configure. Since 2906 it is dropped
   with a warning, correctly for a value nothing passes any more.
4. Nothing then set the new symbol, so `.config` kept whatever the files said:
   - a bringup that states a locator (the Rust workspaces' `10.0.2.2:74xx`)
     kept that, and `nros-zephyr-build`'s `bake_nros_config` baked it;
   - one that states none kept Kconfig's default 7447, which the C define
     baked.

Measured on the tier-2 runner tree (`build-ws-rs-mh-robot1-entry-zenoh`):
- the signature was dated 2026-11-22, before the rename reached the runner;
- `CMakeCache.txt`, re-configured 2026-11-24 08:25Z, holds no `CLI_CONFIG_` for
  either spelling;
- `zephyr/.config` holds the bringup's `tcp/10.0.2.2:7469`.

## Fix

The define is spelled once and the signature records it
(`zenoh_locator=-DCONFIG_NROS_LOCATOR="…"`).
- A rename of the carrier now sends exactly the leaves that carry it through
  west, once.
- A leaf with no locator keeps `zenoh_locator=`, so its signature does not move.

The class is "the signature records a value, but the build is fed a spelling".
The other two carriers (`CONFIG_NROS_XRCE_AGENT_PORT`,
`CONFIG_NROS_DOMAIN_ID`) still record only their value. Neither was renamed,
and recording their spelling would rebuild every XRCE and Cyclone leaf once;
the next rename of either must move the signature with it.

## Acceptance

- The release run-matrix after the fix merges: the five cells above pass.
- Their guests print `locator tcp/127.0.0.1:<allocated port> (baked)`.

## Resolved (2026-11-26)

The fix merged in PR #4790. Both release run-matrix runs since then pass all
five cells in tier 2 (1-wise):

| Run | main | `entry_matrix` | `realtime_sched_tasks` | `sched_dims_applied` | `zephyr_ctf_markers` | `multihost` |
| --- | --- | --- | --- | --- | --- | --- |
| 45533259451 | 939ac840b2 | PASS | PASS | PASS | PASS | PASS |
| 45596628888 | cffdca5c9b | PASS | PASS | PASS | PASS | PASS |

Run 45596628888's tier 2 nightly (pairwise) passes `entry_matrix`,
`zephyr_ctf_markers` and `multihost` too.

- **The second acceptance line** (each guest prints
  `locator tcp/127.0.0.1:<allocated port> (baked)`) is not in these logs.
  The harness prints a guest's console only for a failing test. The cells
  that dial the allocated router passing is the same evidence: on the
  pre-fix run every one of them failed to open a session.
- **What this run still fails is other work.** Run 45596628888's tier 2 is
  red in `sched_dims_applied_e2e`'s per-RTOS census tests and in
  `multihost_partition_bake multihost_bake_emits_only_the_hosts_node`. None
  of those dial a router through the renamed define.
