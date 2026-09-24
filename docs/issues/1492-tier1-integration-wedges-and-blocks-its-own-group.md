---
id: 1492
title: "The tier-1 integration job WEDGES after filling the disk, and its
  concurrency group has `cancel-in-progress: false` and no `timeout-minutes` — so
  one run takes the whole lane offline for hours, not just itself"
status: open
type: bug
area: ci, testing
severity: high
found: 2026-09-24
related: [1353, 1158, 1365]
---

## What happens

`host-tests.yml`'s `nros-tests integration (host)` job runs out of disk inside
`just ci tier1` (that part is issue **1353**). What this issue is about is what
happens NEXT: the job does not end. It fails the tier step, writes its
after-report, and then **hangs on its upload steps**, holding its concurrency
group — and because that group is shared by every push to `main` and configured
`cancel-in-progress: false`, **no later integration job can start at all.**

Measured, 2026-09-24:

Run **36051691975**, job **107808557432**, head `95693968d`, created 19:58 UTC.
At 22:53 — **2 h 55 m later** — it is still `in_progress`, with `updated_at`
frozen at **20:35** (2 h 18 m stale). Its steps:

```
11 Build workspace fixtures                    = success
12 Disk report (before ci tier1)               = success
13 Reclaim disk before the tier                = success
14 Upload the disk transcript (before the tier)= success
15 just ci tier1                               = failure
16 Disk report (after ci tier1)                = success
17 Upload the disk transcript (after the tier) = success
18 Report skipped fixtures (post-run)          = (none)
19 Upload nextest JUnit artifact               = (none)
20 Upload fixture and tier logs                = (none)
39 Post Checkout nano-ros                      = (none)
40 Stop containers                             = (none)
```

Steps 18–20 never conclude. The after-report they follow says
`100% used, 84K free`, so the most likely reading is that the uploads cannot
write their staging copies — but the mechanism of the hang is NOT established
here, only that it hangs with the disk full.

Meanwhile, `host-tests` on `c84fbc73a` (created 22:02) shows
`workspace unit tests = success` and `nros-tests integration (host)`
**`pending` for 51 minutes with no job started.** The runs at 21:24, 21:34 and
21:39 show their unit halves `cancelled` by superseding while their integration
halves never ran at all.

## Why one wedge blocks everything

`.github/workflows/host-tests.yml`:

```yaml
  name: nros-tests integration (host)
    concurrency:
      group: host-tests-integration-${{ github.ref }}-${{ github.event_name }}
      cancel-in-progress: false
```

The key is `ref` + `event_name`, which is **identical for every push to
`main`**, and `cancel-in-progress: false` means a newer run may not displace an
older one. That pairing is deliberate and right for a heavy lane — the comment
one job up says so, and issue 1158 records the same reasoning for tier 2. It is
only dangerous in combination with the third fact:

**the workflow sets no `timeout-minutes`** (`grep` finds none), so a wedged job
runs to GitHub's default **6-hour** limit. For those six hours the lane is not
slow, it is **offline**.

## What this is NOT

- **Not issue 1353.** 1353 is "the tier needs ~24.8 GiB after the reclaim and
  does not get it", reproduced three times within 10 MB, with the growth
  attributed to `packages/cli/target` (409 MB → 14 GB during the tier). Fixing
  the disk would stop this wedge from being REACHED, but the wedge-plus-no-timeout
  shape would still be there for the next thing that hangs.
- **Not issue 1365.** That is tier-2 jobs waiting on a self-hosted runner that
  was not registered. This job is `runs-on: ubuntu-22.04`, hosted, and the wait
  is its own group, not a missing runner.
- **Not superseding working as intended.** The unit half DOES supersede
  correctly (`host-tests-unit-…`, and the comment there explains the fix that
  made it work). The integration half is the one that cannot.
- **Not a claim that the uploads are the cause.** Steps 18–20 are where it
  stops; why they never return is unmeasured.

## What would close it

1. **A `timeout-minutes` on the integration job**, so a wedge costs one run's
   budget instead of six hours of lane. This is the cheap half and it bounds the
   damage whatever the cause.
2. **Uploads that survive a full disk**, or a reclaim before them — steps 18–20
   run only after the tier has already consumed everything, which is the worst
   possible moment to ask for scratch space.
3. Optionally, a group key that lets a newer push supersede a run that has
   already FAILED its tier step, since nothing downstream of that failure is
   worth the lane.

Acceptance: a `host-tests` push run that fails the tier CONCLUDES, and the next
push's integration job starts without waiting on it.
