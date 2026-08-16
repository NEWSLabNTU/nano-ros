---
id: 651
title: "The Zephyr 4.4 line is reachable only from nightly, so a Kconfig or API change lands unverified for a day"
status: open
type: tech-debt
area: build/zephyr
related: [issue-0078, issue-0626]
---

## Symptom

Nothing a developer can run before pushing touches Zephyr 4.4. `just ci` (tier 1),
`just ci-matrix` (tier 2) and `just ci-full` (tier 3) never set
`NROS_ZEPHYR_VERSION`, which defaults to `3.7` in `just/zephyr.just:10`, so every
local lane resolves `west.yml` and the 3.7 workspace. The 4.4 line exists only in
`.github/workflows/nightly.yml`, gated on `needs.changes.outputs.zephyr == 'true'`
and the `0 5 * * *` schedule:

- `zephyr-matrix` — `line: ["3.7", "4.4"]` × 10 examples, each
  `just zephyr build-one <example> zenoh`, so 10 of the 20 cells are 4.4.
- `zephyr-dual-line-summary` — `just zephyr ci-both`, one representative example
  per line.
- a 4.4-only copy-out check.

So the feedback loop for anything 4.4-specific is a day, and only if the change
touched a path the `changes` filter counts as `zephyr`.

## Why this is more than "a lane we do not run locally"

The two lines are not the same build in different clothes. They diverge in ways
that make 3.7 a poor proxy:

- **Different Kconfig option sets.** Zephyr's POSIX options were reorganised
  across these releases, and a symbol present in one line can be absent in the
  other. A `select` of a symbol that does not exist is a Kconfig *warning*, not
  an error, so the 4.4 half of such a change fails quietly rather than loudly.
- **A different patch set.** `scripts/zephyr/patches/4.4.sh` applies three NSOS
  patches re-anchored to the 4.4 source shape plus a `pthread_mutex_unlock`
  relaxation (4.x `k_mutex` is owner-only-unlock and aborts ddsrt's xevent thread
  after the first publish). None of that is exercised by 3.7.
- **A different Python floor and workspace.** 4.4 needs a 3.12 venv and lives in
  a separate sibling workspace (`../nano-ros-workspace-4.4`), so a developer with
  a working 3.7 setup has no 4.4 setup at all.
- **`ci-both` SKIPS a line whose workspace is absent** (by design — a skip with a
  message beats a silent pass). On a machine with only the 3.7 workspace, the one
  recipe that names both lines reports success having built one.

`native_sim`, NSOS and driver source are explicitly *not* stable Zephyr surfaces
(phase-199), which is exactly why the per-line patch sets exist. The version
skew is the point of having a rolling line; the absence of a pre-push lane over
it is the gap.

## How it showed up

Issue 0626's Zephyr priority map calls `sched_get_priority_min/max`. Those are
compiled only under `CONFIG_POSIX_PRIORITY_SCHEDULING`, which `CONFIG_POSIX_API`
does not select, so the 3.7 build died at link. The fix states the dependency in
`zephyr/Kconfig` with `select POSIX_PRIORITY_SCHEDULING`, verified on 3.7 by
building `rust/talker` and confirming `sched.c.obj` and both symbols.

**The 4.4 half of that fix is unverified**, and could not be verified before
pushing: confirming it needs a 4.4 workspace, which is a full west fetch plus a
py312 venv, and no such workspace exists on a normal dev host. If
`POSIX_PRIORITY_SCHEDULING` is spelled differently — or absent — in 4.4's
Kconfig, the `select` is a warning that reaches all ten 4.4 matrix cells the
following night, attributed to whatever else moved that day.

That is the shape of the problem: not that 4.4 is broken, but that a change can
be *correct on the line you can run* and unknown on the line you cannot.

## Why the rolling line exists (checked, because memory disagreed)

The recollection that 4.4 was added because *a feature requires it* is not what
the record says, and the distinction matters for how much this gap costs.

Per phase-199 and `docs/development/zephyr-version-support.md`, the 4.4 line is a
**policy** artifact, not a feature dependency: support is bounded by
`zephyr-lang-rust` (whose first commit post-dates the 3.7 LTS, which is why 3.7
is a hard floor and ASI's 3.6 pin cannot work), and the declared window is
"current LTS as default/CI baseline + **at most one rolling line**". 4.4 is that
rolling line. `65c1998a4`, the commit that pinned `west-4.4.yml`, describes
bringup only and names no feature.

No feature in this tree is gated on 4.4 — nothing selects a manifest by version,
and the 4.4-specific work in the issue record is all *keeping the line building*
(0058 libcpp `initializer_list`, 0078 setup ENOSPC), never a capability that 3.7
lacks.

If a 4.4-requiring feature does exist and predates this note, correct this
section — it changes the priority, because then the untested line is one the
product depends on rather than one the policy maintains.

## Directions

Not diagnosed further; these are candidates, not a plan.

- **A cheap pre-push signal that does not need a workspace.** Most of what breaks
  here is *symbol existence*, which is answerable from the pinned Zephyr tree
  alone. A gate that resolves every `select`/`depends on` in `zephyr/Kconfig`
  against both manifests' Zephyr revisions would have caught the case above
  without building anything. It needs a checkout, not a build, and could be
  nightly-cheap or on-demand.
- **Make the 4.4 workspace obtainable in one step and say so**, so "verify on
  4.4" is a documented action rather than a research task. Note 0078: the 4.4
  setup is the heavier of the two and has run a CI host out of disk.
- **Promote a single 4.4 cell into tier 3.** `ci-full` is already pre-release and
  on-demand, so the latency cost lands where it is affordable — but only if the
  workspace question above is solved first, or it will SKIP and read as green.
- **Decide the line's status deliberately.** If no feature requires 4.4 and the
  gap is not worth closing, then dropping the rolling line is a legitimate
  answer, and cheaper than half-testing it. The support policy allows "at most
  one rolling", not "at least one".

Whichever way it goes, the failure mode to design against is the one above: a
lane that skips when unprovisioned and reports the same colour as a lane that
passed.
