---
id: 1082
title: "A changed Kconfig `default` never reaches a reused Zephyr build dir, so 35 images ran the transport at the LEAST urgent priority — issue 0852's symptom, five days after 0852 was fixed"
status: resolved
type: bug
area: build
severity: high
found: 2026-09-05
related: [0852, 0623, 0196, 1075, 1080]
---

# Kconfig keeps what it has; a new `default` is not a new value

## Symptom

`check-tier-priority-plan-image` fails across every realtime workspace:

```
tiers.high.zephyr = 9 is MORE URGENT than the transport band [14, 14] and does not say so.
  Move it into pool.app [15, 14], or state the choice with `above = "transport"`.

tier-priority-plan-image: FAILED (0 pin-check(s) over 90 image(s))
```

**The remedy it offers cannot be followed.** `pool.app` is `[15, 14]` — an EMPTY
range. The resolver computes it as `(ks[-1] + 1, num_preempt - 1)`
(`scripts/lib/priority_plan.py`), and with the transport band sitting on `14`
and `CONFIG_NUM_PREEMPT_PRIORITIES=15` that is `(15, 14)`. There is no
priority less urgent than the transport to move a tier into.

## Cause — not the workspaces, not the resolver

Neither the `system.toml` files it names nor the resolver is wrong. The
transport is in the wrong place:

```
detail: read  { band: 16, posix: 0, kthread: 14 }
        lease { band: 16, posix: 0, kthread: 14 }
```

`band: 16` is the OLD 0-31-scale value. Issue 0852 replaced that scale with the
platform ABI's normalised 0-255 band and moved the default to **200**, and its
own Kconfig help predicted this failure exactly:

> **THE SCALE CHANGED, AND THE OLD VALUES DO NOT CARRY OVER.** … a 16 authored
> against 0-31 is `(16*14)/255 = 0` through the real map — the LEAST urgent
> slot, which is exactly where every Zephyr transport task has been running.

So the transport ends up at kthread 14, the bottom of the preempt range, and
every application tier is necessarily more urgent than it.

## Why it survived the fix

**Nothing in the tree sets 16.** `git grep` finds no `.conf` fragment and no
cmake site; `zephyr/Kconfig:434` says `default 200`.

The value comes from the BUILD DIRECTORY. Zephyr's Kconfig preserves an existing
`.config` across re-configures — a `default` applies only to a symbol that has no
value yet — so a build dir created before the scale change carries `16` forward
forever, including through a full rebuild into the same directory. The
`.config.old` sitting beside it is the mechanism in plain sight.

Measured 2026-09-05: **35 of the images on this host** carried
`CONFIG_NROS_ZENOH_READ_PRIORITY=16`, the oldest from 2026-08-31 — **and one of
them was rebuilt TODAY**, which is what rules out "just stale directories" and
points at the carry-forward.

This is the issue-0196 class (a build-side artifact outliving the source it was
configured from) applied to Kconfig rather than to a fixture: no mtime check
catches it, because the `.config` is NEWER than the Kconfig that would have
changed it.

## Why it was invisible until today

The gate that reports it needs BUILT Zephyr images, and it says so — the static
`check-tier-priority-plan` passes and DEFERS these 8 pins to
`check-tier-priority-plan-image`. No Zephyr image could be built on this host at
all until issues **1075** (link) and **1080** (compile) were fixed hours earlier.
Fixing those two is what let this check run for the first time.

Three failures stacked in one lane, each hiding the next.

## Fixed 2026-09-05

Deleted the stale `.config` / `.config.old` from the 35 affected build dirs, so
Kconfig re-derives from the current default. Verified on
`build-cortex-m-c-talker-zenoh`:

```
before:  CONFIG_NROS_ZENOH_READ_PRIORITY=16   (band 16 -> posix 0 -> kthread 14)
after:   CONFIG_NROS_ZENOH_READ_PRIORITY=200
         cmake --build -> rc=0
```

Surgical rather than `rm -rf` on the build dirs: CLAUDE.md's rule is that wiping
destroys the evidence and hides the missing edge. Only the generated `.config`
is removed, which is the one file carrying the stale value.

## Not covered — and this is the part worth a follow-up

**The carry-forward is a general hazard, not a one-off.** Any Kconfig `default`
this repo changes fails to reach every pre-existing build dir, silently, and the
symptom appears far from the cause — here as a scheduling-plan violation blamed
on four workspaces' `system.toml`.

Nothing detects it. Candidates, none implemented:

* a `.config`-vs-Kconfig staleness probe in the fixture lane, the way build-side
  probes already watch `generated/**` (issue 0196);
* the west build refusing a `.config` whose Kconfig inputs are newer;
* `check-kconfig-overridden-values` extended from "a later fragment overrides a
  leaf value" to "a build dir overrides the tree".

Also unexamined: whether other `CONFIG_NROS_*` defaults changed since these
build dirs were created, and how many carry stale values that no gate happens to
inspect. Only the two priority symbols were checked, because only they had a
gate looking.
