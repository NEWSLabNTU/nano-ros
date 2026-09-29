---
id: 1583
title: "`tier-priority-plan-image` fails the derived Zephyr image's lowered band [4, 9] against the realtime bringups' `tiers.high.zephyr = 9`"
status: resolved
type: bug
area: [zephyr, codegen, testing]
severity: medium
found: 2026-09-29
related: [issue-1508, issue-1537, issue-1571, issue-1582, issue-1536, issue-1226]
resolved_in: "branch fix/1583-tier-priority-plan-image"
---

## What happens (reported by the issue-1571 agent, not yet re-measured)

The Zephyr post-build gate `tier-priority-plan-image` fails on the derived
realtime image. The image's lowered priority band is [4, 9], and the gate
checks it against the authored `tiers.high.zephyr = 9` in the realtime-c,
realtime-cpp and realtime-rust bringups. None of those `system.toml` files
were changed by the 1571 work, so this appears to be the state on `main`
since #1508 (baked priority plan) and the #1537 derived-tier lane.

## To decide

Which side is wrong: the gate's reading of an authored priority against a
derived band, the band, or the bringups' authored value? #1508 made the
image's own `.config` plan the source of priorities, and an authored raw
priority at the band edge may simply be illegal now. If so, the gate is right
and the bringups need updating.

## Acceptance

- The gate passes on every realtime bringup's Zephyr image, or the bringups'
  values are corrected with the reason recorded.
- A merge-gating or nightly lane runs the gate, so it does not sit red unseen.

## Resolution

**The GATE was wrong; the band and the authored values are right.**

Reproduced on the five realtime images built in this worktree (the pre-fix
script over them):

```
  [ok]   build-ws-cpp-realtime-entry-zenoh: transport [4, 4], pool [5, 14] — 8 pin(s)
  [ok]   build-ws-c-realtime-entry-smp: transport [4, 4], pool [5, 14] — 8 pin(s)
  [ok]   build-ws-c-realtime-entry-zenoh: transport [4, 4], pool [5, 14] — 8 pin(s)
  [FAIL] build-ws-rs-realtime-derived-entry-zenoh: transport [4, 9], pool [10, 14]
        examples/workspaces/realtime-c/src/demo_bringup/system.toml: tiers.high.zephyr = 9 lands ON the reserved transport band [4, 9]
        examples/workspaces/realtime-c/src/smp_bringup/system.toml: tiers.high.zephyr = 9 lands ON the reserved transport band [4, 9]
        examples/workspaces/realtime-cpp/src/demo_bringup/system.toml: tiers.high.zephyr = 9 lands ON the reserved transport band [4, 9]
        examples/workspaces/realtime-rust/src/demo_bringup/system.toml: tiers.high.zephyr = 9 lands ON the reserved transport band [4, 9]
  [ok]   build-ws-rs-realtime-entry-zenoh: transport [4, 4], pool [5, 14] — 8 pin(s)
tier-priority-plan-image: FAILED (36 pin-check(s) over 5 current image(s); 0 STALE, 0 NO BAND)
```

**What the band is.** `resolve_zephyr_plan` maps each zenoh task's Kconfig
band (0-255) through `nros_zephyr_native_priority` to a POSIX priority and then
`POSIX_TO_ZEPHYR_PRIORITY(SCHED_RR)` to a k_thread priority, on
`CONFIG_NUM_PREEMPT_PRIORITIES=15`. Both gates
(`CONFIG_POSIX_PRIORITY_SCHEDULING`, `CONFIG_PREEMPT_ENABLED`) are on, so the
band is applied, all preemptive (cooperative levels are the negatives,
`range = (-16, 14)`):

| image | read band | lease band | transport | pool.app |
| --- | --- | --- | --- | --- |
| realtime c/cpp/rust (+SMP) | 200 → k 4 | 200 → k 4 | [4, 4] | [5, 14] |
| `derived_bringup` (`prj-lowered-band.conf`) | 100 → k 9 | 200 → k 4 | [4, 9] | [10, 14] |

The derived image lowers the READ task on purpose (#1537: on a default band the
projection and the image's plan agree, so a regression could not be seen).

**Why the gate was wrong.** A band is a property of ONE image; a pin is a
property of ONE bringup. The gate resolved the first correctly and then judged
every `[tiers.*.zephyr]` pin in the TREE against it. `derived_bringup` authors
no pin at all (its tiers are derived, and land at 10 / 11). The four failing
pins belong to images whose band is [4, 4], where 9 is a pool priority strictly
below the transport. Measured at runtime, which is what a static gate stands in
for:

- the realtime-c image, booted against a router: no `tier priority meets the
  transport band` report and no `NOT checked` line — i.e. the transport tasks
  were created at explicit priorities and `nros_zephyr_report_tier_vs_transport`
  found tier 9 > floor 4 (`zephyr/nros_platform_zephyr_shims.c`);
- the derived image: `sched_dims_applied` —
  `[zephyr rust DerivedTierBelowTransport] SILENT (derived tiers below the transport)`.

Nothing on either image ever runs a tier at 9 against [4, 9].

**Fix** (three commits on `fix/1583-tier-priority-plan-image`):

1. `check-tier-priority-plan-image.py` attributes each image to the bringup it
   was built from, through the `examples/fixtures.toml` row that names its build
   dir (read via `fixtures-manifest.py`'s own `west_build_name`), and judges
   only that bringup's pins. NOT the image's `nros-image-facts.cmake`: it
   resolves from the ENTRY package and reads `demo_bringup:zephyr` on the
   derived image (issue 1582's shared entry). A listed dir no row names FAILS;
   a pinned bringup no current image was built from prints NOT JUDGED. The
   selftest holds the attribution to the real tree (every pinned bringup has a
   Zephyr row — mutation-checked by pointing the SMP row at `demo_bringup`),
   and runs on the FAST line as `check::tier-priority-plan-image-selftest`.
   RFC-0079 §4.1 states the rule.
2. The gate REPORTS on a red lane. It sat as the LAST step of
   `zephyr::build-fixtures`, after `west-fixtures.sh`, so a failing leaf or west
   fixture ended the recipe first. It is placed on the tier-2 nightly
   (`just build tier2-nightly` builds exactly `build-ws-rs-realtime-entry-zenoh`
   and the derived image), and nightly 36391905926 died on an unrelated zephyr
   leaf before reaching it; locally every run dies at issue 1536's
   `zephyr_self_pkg_sibling` after the realtime leaves built. Now it runs right
   after the leaves, judges the leaves whose driver joblog row is `ok` when
   some failed (`zephyr-fixture-make-driver.sh --joblog`), and the recipe exits
   with the first of the driver / gate / west-fixtures statuses.
3. The four `[tiers.high.zephyr]` comments named the pre-0852 band ("7, with
   pool [8, 14]"); they now name the measured [4, 4] / [5, 14]. No value moved.

**Acceptance, measured** (images built from `00fb35c8a`/`357bec2ea`'s tree):

```
check-tier-priority-plan-image: 5 image(s) (listed in .../tier-priority-images-3181121.txt)
  [ok]   build-ws-rs-realtime-entry-zenoh: transport [4, 4], pool [5, 14] — 2 pin(s) from examples/workspaces/realtime-rust/src/demo_bringup/system.toml
  [ok]   build-ws-rs-realtime-derived-entry-zenoh: transport [4, 9], pool [10, 14] — 0 pin(s) from examples/workspaces/realtime-rust/src/derived_bringup/system.toml
  [ok]   build-ws-cpp-realtime-entry-zenoh: transport [4, 4], pool [5, 14] — 2 pin(s) from examples/workspaces/realtime-cpp/src/demo_bringup/system.toml
  [ok]   build-ws-c-realtime-entry-zenoh: transport [4, 4], pool [5, 14] — 2 pin(s) from examples/workspaces/realtime-c/src/demo_bringup/system.toml
  [ok]   build-ws-c-realtime-entry-smp: transport [4, 4], pool [5, 14] — 2 pin(s) from examples/workspaces/realtime-c/src/smp_bringup/system.toml
tier-priority-plan-image: OK (8 pin-check(s) over 5 current image(s); 0 STALE, 0 NO BAND, 0 NO ROW)
west fixtures: 4/5 ok ...        (issue 1536 — still reported, recipe rc=1)
```

A deliberately bad value is still refused (realtime-rust `high` set to 4, then
3, then restored):

```
  [FAIL] build-ws-rs-realtime-entry-zenoh: transport [4, 4], pool [5, 14] from .../demo_bringup/system.toml
        ...: tiers.high.zephyr = 4 lands ON the reserved transport band [4, 4]
tier-priority-plan-image (zephyr): FAILED
  ...: tiers.high.zephyr = 3 is MORE URGENT than the transport band [4, 4] and does not say so.
      Move it into pool.app [5, 14], or state the choice with `above = "transport"` on [tiers.high].
```

Runtime: `realtime_tiers` — 3 Zephyr rows (rust, c, cpp) ran and PASSED (14
other-platform rows skipped, fixtures not built here). `sched_dims_applied` —
`DerivedTierBelowTransport` SILENT, c/cpp CorePinPlacement/EdfDeadline ACCEPT;
`CorePin/zephyr/rust` and `EdfDeadline/zephyr/rust` FAIL because the
`build-ws-rs-realtime-entry-zenoh` image boots `boot tier derived-telem_node` —
it carries `derived_bringup`'s table. That is issue 1582 (the two rows share
one generated `build/zephyr-zenoh/zephyr_entry`), not this issue.
