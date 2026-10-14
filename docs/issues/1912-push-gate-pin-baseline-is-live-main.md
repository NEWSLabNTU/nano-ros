---
id: 1912
title: "A push-event gate compares submodule pins against the LIVE `origin/main`, so a commit superseded by a later pin bump reads as a REWIND"
status: open
type: bug
area: [ci]
severity: low
found: 2026-10-14
related: [1043]
---

## Measured

Push-event gate **38646258842** for main `d4ed5af76` (#1894), created at
22:14Z. Two jobs failed in `check fast`, with the same gate:

- **116134838467** `check / fast (every event)`
- **116134742937** `warm the compile-tier cache (main only)`

```
===== FAIL (submodule-pins, rc=1, 864ms) =====
submodule-pins: third-party/nuttx/libc
    REWIND — the new pin is an ANCESTOR of the old one
      was: 994735191a4b  Size NuttX sem/mutex/cond/rwlock mirrors for priority inheritance (nano-ros #1907)
      now: 826c4ca918d8  Size pthread_attr_t for CONFIG_SCHED_SPORADIC (nano-ros #570)
```

**No pin moved backward.** Main is linear: `d4ed5af76` (22:01:35Z) is an
ancestor of `eadf501a4c` (#1907, 22:04:47Z), and #1907 is the commit that moved
`third-party/nuttx/libc` **forward** from `826c4ca918d8` to `994735191a4b`.
`d4ed5af76` changed no pin at all.

## Cause

`gate.yml`'s "Fetch the pin baseline for check-submodule-pins" step (five
copies, one per job: lines 663, 981, 1292, 1662 and 1927 as of `843ced628`) does:

```bash
base="${GITHUB_BASE_REF:-main}"
git fetch --no-tags --quiet --depth=1 origin "+refs/heads/$base:refs/remotes/origin/$base"
echo "NROS_SUBMODULE_PIN_BASELINE=origin/$base" >> "$GITHUB_ENV"
```

On a pull request or merge group the baseline is the branch the change goes
INTO, which is correct. On a **push to main** it is main's tip **at the moment
the job runs**. Push gates queue behind each other, so by the time a job runs
for an older commit, main has often moved past it. Any pin that later commit
moved forward then compares as `new = this commit's pin` against
`old = a newer main's pin`, which reads as a rewind.

The failure is false and self-clearing: the next push gate, for the commit
that made the bump, compares correctly. But it turns a green commit red and
reads exactly like the defect the gate exists to catch.

## What this is NOT

- **Not a real rewind.** The gate's own output says `826c4ca918d8` is an
  ancestor of `994735191a4b`, and main's current pin is `994735191a4b`, the
  newer one.
- **Not 1043** (no object store, so NOT VERIFIED). Here both commits were
  present and the gate measured, against the wrong baseline.

## What would close this

On `push`, compare against the commit the push **replaced**, not against live
`main`. `github.event.before` is that SHA. Fetch it with `--depth=1` (GitHub
serves a fetch by SHA) and set `NROS_SUBMODULE_PIN_BASELINE` to it. Keep
`origin/$GITHUB_BASE_REF` for `pull_request`, and `main` for `merge_group`. The
five copies should become one shared step, or a composite action, so the
events cannot drift apart again.
