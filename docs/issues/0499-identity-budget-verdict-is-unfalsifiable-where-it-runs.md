---
id: 499
title: "`check-artifact-identity-budget` runs only where its reading cannot be trusted, and skips where it can — a stale tree fails tier 1 at step one"
status: open
type: tech-debt
severity: medium
area: build
related: [issue-0485, phase-340, issue-0446]
---

## Symptom

`just ci` (tier 1) failed at the FIRST gate, on a tree nobody had touched:

```
artifact-identity budget: FAIL
  nros_core has 12 distinct -C metadata identities in
  examples/workspaces/mixed/build-workspace-fixtures
  (budget 4, recorded 2026-08-07 by phase-340 W4).
```

The build directory was last written **2026-08-07**; the run was 2026-08-10, on
a diff touching only test sources, docs and one shell script. `check-fast` is
the first thing `ci` runs and it stops at the first failure, so **every later
step — the whole build tier, clippy, `test-all` — never ran.** The change could
not be gated at all until the tree was dealt with.

Deleting `examples/workspaces/mixed/build-workspace-fixtures` and rebuilding
(`just build-test-fixtures lane=native`) cleared it. The count was history.

## This is not the gate being wrong — it is the gate being unfalsifiable here

The script already documents the accumulation risk and accepts it deliberately
(`scripts/check-artifact-identity-budget.sh`, "WHEN A WORK ITEM LANDS…"):

> It is deliberately NOT wired into `build-test-fixtures`: a long-lived
> incremental tree ACCUMULATES rlibs from earlier builds (cargo never collects
> them) … Failing a static check, whose remedy is "delete the tree and
> rebuild", is survivable.

Two things about that reasoning did not survive contact:

**1. The coverage is inverted.** The same comment notes that on a pristine CI
checkout there is no tree, so the gate SKIPS there and "its live coverage is the
local one". But the local tree is precisely the one that accumulates. So the
gate reports only where its reading is least trustworthy, and stays silent
where it would be exact. A real regression and three days of history produce
the identical message.

**2. "Delete the tree and rebuild" is not survivable at tier-1 cadence.** Tier 1
is the lane CLAUDE.md says to run *after every task*. The remedy here is a full
native fixture rebuild — measured at tens of minutes on a 24-core host, and it
also re-stales everything keyed on the CLI stamp. A gate that can demand that,
from the first step of the lane meant to be cheap, will be bypassed
(`NROS_SKIP_*`) rather than obeyed — which is the outcome the script's own
"turned off within a week" argument was trying to avoid. It moved the failure
from the build to the check without changing its cost.

**3. Wiping is also how a REAL regression gets erased.** The prescribed
response to a red is to destroy the evidence and re-measure. If the count was
genuine, the rebuild silently fixes the symptom and the regression ships. The
verdict cannot be acted on correctly because it cannot be interpreted.

## Option 1 needs a field the stamp does not have (attempted 2026-08-10)

Tried the preferred option — "read only artifacts from the current build",
filtering rlib mtimes against the fixtures stamp. **It cannot work as written,
and the failure is silent in the dangerous direction: the gate SKIPS every
time**, which is worse than the over-counting it replaces.

`built_at` is written when the build FINISHES, not when it starts. Measured on
a tree from one `lane=all` run:

```
artifacts   2026-08-10T23:22:05 .. 23:23:17   (local)
built_at    2026-08-10T15:31:21Z  =  23:31:21 (local)
```

So every artifact the build produced is OLDER than the stamp, and the filter
classifies the whole current build as history. Filtering against the stamp
FILE's mtime fails identically, for the same reason.

`built_at` is an upper bound on the build's output; the filter needs a lower
one. The smallest fix is a `started_at=` line written at the top of the run
(the stamp writer already composes the file, so it is one more line), after
which the filter is exactly the one-liner this issue proposed. Without it,
option 1 has nothing correct to compare against.

**Reverted, not shipped.** A gate that always skips reports green forever, which
is the failure this issue is about, inverted — the count would stop being
untrustworthy by ceasing to exist. Options 2 and 3 are unaffected by this and
remain open.

## Direction

The gate needs to tell accumulation from a regression instead of asking the
operator to guess. Options, roughly in order of preference:

1. **Read only artifacts from the current build.** The stamp
   (`target/nextest/.fixtures-built`) records when the lane was built; rlibs
   older than it are history by definition and should not be counted. This is a
   filter on mtime against a value the repo already writes, not new machinery.
2. **Run it where the tree IS pristine** — as a post-step of
   `build-test-fixtures`, reporting rather than failing, so the number is
   recorded from a known-clean tree and drift is visible over time.
3. **Fail with the diagnosis, not the count.** If the identities split cleanly
   by mtime era, say so ("8 of 12 predate this build") instead of printing all
   twelve and a paragraph asking the reader to decide.

Whatever the mechanism: a gate whose red requires a 40-minute wipe before it
can be believed is a gate whose red will be ignored. Issue 0485 already found
this gate reporting a wrong number for a different reason (locale collation
splitting one crate in two) and its lesson applies again — nothing about a
wrong reading here looks wrong.

## Reproduce

Build the mixed workspace fixtures, do unrelated work for a few days across a
`--target`-spelling or profile change, then:

```sh
source ./activate.sh
bash scripts/check-artifact-identity-budget.sh
```

It reports over-budget for crates whose extra identities are all older than the
current build.
