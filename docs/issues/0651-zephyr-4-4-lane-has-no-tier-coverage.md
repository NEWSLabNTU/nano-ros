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

## Partly addressed (2026-08-20) — the symbol-existence half is now gated

`scripts/check-zephyr-kconfig-symbols.py` (`just check-zephyr-kconfig-symbols`)
resolves every `select` / `imply` / `depends on` in `zephyr/Kconfig` against the
symbols DEFINED by each supported line, and fails on one that is absent. This is
the first direction listed below: it needs SOURCE, not a build, and runs in ~2 s.

**The 4.4 tree does not need a west workspace.** Two bare clones are enough —
the `zephyr` repo at `v4.4.0` and `zephyr-lang-rust` at the SHA `west-4.4.yml`
pins — which sidesteps issue 0078's disk exhaustion entirely:

```
git clone --depth 1 --branch v4.4.0 --single-branch \
    https://github.com/zephyrproject-rtos/zephyr build/zephyr-kconfig/zephyr-4.4
git clone https://github.com/zephyrproject-rtos/zephyr-lang-rust \
    build/zephyr-kconfig/zephyr-lang-rust-4.4   # then `git checkout <west-4.4.yml pin>`
```

The module clone is not optional and was this gate's first false positive:
`RUST` is defined by `zephyr-lang-rust`, not by zephyr, so a zephyr-only walk
reports it missing on every line. A line's symbol universe is zephyr PLUS the
modules west pins.

**The motivating unknown is now answered.** This issue was filed partly because
`select POSIX_PRIORITY_SCHEDULING` (issue 0626) was verified on 3.7 and
unverifiable on 4.4. It is present on BOTH, at the same path —
`lib/posix/options/Kconfig.sched:7`. The 0626 fix is correct on the rolling line.

Verified adversarially: renaming that symbol in `zephyr/Kconfig` makes the gate
report it absent on both lines, with the file and line number; reverting
restores OK. Checking NO line is a hard failure, not a pass — the gate refuses to
report success having measured nothing (issue 0702).

## What this does NOT cover

Symbol EXISTENCE only. A symbol that exists on both lines but means something
different, a patch set that applies cleanly to one shape and not the other, the
py312 floor, `k_mutex` owner-only-unlock — none of that is answerable without
building. The nightly is still the only thing that builds 4.4, so the day-long
feedback loop stands for everything except spelling.

The remaining directions below are unchanged.

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

## 2026-08-20 (second pass) — the gate was green on every host that lacked 4.4

The symbol gate landed above (`08e421df6`) and its own account of itself was
accurate but narrower than the rule this issue states. Measured on a normal dev
host, before any change:

```
$ just check-zephyr-kconfig-symbols ; echo rc=$?
zephyr-kconfig-symbols OK — 11 referenced symbol(s), lines checked: 3.7 (3.7)
  NOT checked: 4.4 — no tree present. …
rc=0
```

**OK, exit 0, having looked at neither symbol on the line the gate exists for.**
The code hard-failed only on `if not trees:` — zero lines present. With 3.7 from
`just zephyr setup` and no 4.4 anywhere, which this issue itself describes as
every ordinary dev host ("a developer with a working 3.7 setup has no 4.4 setup
at all"), it reported success. That is verbatim the failure mode named at the
bottom of this issue: *a lane that skips when unprovisioned and reports the same
colour as a lane that passed.*

The claim "Checking NO line is a hard failure, not a pass" was true and is not
the same statement.

### Direction 2 — the 4.4 source is now one command

`just zephyr kconfig-trees` -> `scripts/zephyr/fetch-kconfig-trees.sh`. Two
shallow clones into `build/zephyr-kconfig/` (gitignored), **613 MB measured**, no
west workspace and so no repeat of issue 0078's disk exhaustion. Revisions are
READ from `west-4.4.yml` — `zephyr` at its tag, `zephyr-lang-rust` at the SHA —
never restated in a second file that could disagree with the manifest. A tag
clones shallow by name; a bare SHA cannot, so that path does an empty clone plus
a single-object fetch, still shallow. Idempotent: a re-run with both trees at
their pins prints `already at` and exits.

### Direction 1's second half — the verdict is now honest

An unchecked SUPPORTED line is a failure, not a footnote. Strictness is only
affordable because the remedy above exists; making it fail first would have been
the trap this issue warns about. `NROS_ZEPHYR_KCONFIG_ALLOW_PARTIAL=1` overrides
it and labels the run `PARTIAL` in the output.

Verified in all four directions rather than the happy path:

| condition | result |
| --- | --- |
| both lines present | `OK … lines checked: 3.7 (3.7), 4.4 (4.4)`, rc=0 |
| 4.4 absent | rc=1, `1 supported Zephyr line(s) were NOT checked: 4.4`, naming `just zephyr kconfig-trees` |
| 4.4 absent + `ALLOW_PARTIAL=1` | rc=0, output labelled `PARTIAL` |
| a symbol renamed in `zephyr/Kconfig` | rc=1, reported `absent on 3.7` AND `absent on 4.4`, each with file:line |

The fourth row is the one that matters for regression: strictness did not
displace the substantive check, and 4.4 is now genuinely covered on this host
rather than nominally.

## What is still open

Directions 3 and 4, unchanged, and both are decisions rather than work:

* **Promote a 4.4 cell into tier 3.** Now unblocked — the workspace question
  that made it "skip and read as green" is answered for the KCONFIG tree. A
  BUILD cell still needs the full west workspace and the py312 venv, so this is
  not free.
* **Decide the line's status.** Nothing in this tree is gated on 4.4; the
  support policy allows "at most one rolling", not "at least one". Dropping it
  remains a legitimate and cheaper answer than half-testing it.

Everything this gate can answer without building is now answered on both lines.
Everything else — a symbol that exists on both but means something different, the
patch sets, `k_mutex` owner-only-unlock, the py312 floor — still needs a build,
and the nightly is still the only thing that does one.
