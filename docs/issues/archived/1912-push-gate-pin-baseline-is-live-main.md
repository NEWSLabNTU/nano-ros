---
id: 1912
title: "A push-event gate compares submodule pins against the LIVE `origin/main`, so a commit superseded by a later pin bump reads as a REWIND"
status: resolved
type: bug
area: [ci]
severity: low
found: 2026-10-14
related: [1043, phase-505]
resolved_in: "branch fix/1373-1912"
---

**Home: [phase-505](../../roadmap/archived/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## Second sighting, two hours later, on a different submodule (2026-10-15)

Push gate **38658728637** for main `87d46c3b2` (#1909, 23:55Z), created at
00:12Z. `check / fast` (job **116174697368**) and the warm-cache job failed
the same way:

```
submodule-pins: packages/cli/third-party/play_launch
    REWIND — the new pin is an ANCESTOR of the old one
      was: c0a6a2e6dd49  fix(ci): the census counts budget_book.rs as a consumer; rustfmt a test
      now: 465dba4d4c1f  chore(phase-86): pin ros-launch-manifest v0.1.57
```

`87d46c3b2` moved no pin. The next commit on main, `f496a8b68` (00:01Z,
"rlm v0.1.58 and play_launch c0a6a2e6"), moved `play_launch` **forward** from
`465dba4` to `c0a6a2e6`, and this job compared against that newer main.

Two in one evening, on two submodules, means any pin bump that merges within
a push gate's queue time turns the previous commit's gate red. That happens
whenever the queue is busy, so it is no longer a rare race.

## Third sighting, a different verdict from the same baseline (2026-10-15)

Push gate **38774904187** for main `5065beb8a` (created 17:09Z) failed in
`check / fast` (job **116560014147**, 17:09:16Z to 17:20:55Z). The pin gate
did not say REWIND this time. It reported NOT VERIFIED, and the skip
allowlist turned that into the failure:

```
check-fast-skip-allowlist: FAIL -- these gates did not verify in CI:
  submodule-pins: PARTIAL — 1 of 1 moved pin(s) NOT VERIFIED (not checked out here): packages/cli/third-party/play_launch
```

`5065beb8a` moved no pin. The next commit on main, `573490978` (#2271),
moved `play_launch` forward and merged while this job was running. The job's
`NROS_SUBMODULE_PIN_BASELINE` was `origin/main`, so it compared this commit's
`play_launch` pin against #2271's newer one. The job checks out `play_launch`
with `--depth 1`, so the older pin's ancestry is not in the clone. That gives the gate's third outcome
(issue 1043) instead of the REWIND of the first two sightings.

Same cause, a second symptom: whether a commit superseded by a pin bump
reads as REWIND or NOT VERIFIED depends on how much submodule history the
job happened to fetch. Either way the gate for a commit that moved no pin
goes red. The fix under "What would close this" covers both.

## Fourth sighting, in the main-only cache-warming job (2026-10-16)

Push gate **38829571901** for main `ba348cc20` (00:27Z) failed in
`warm the compile-tier cache (main only)` (job **116745434424**). That job runs
`check fast` too, so it has the same pin gate:

```
submodule-pins: packages/rmw/zenoh/zpico-sys/zenoh-pico
    REWIND — the new pin is an ANCESTOR of the old one
```

`ba348cc20` moved no pin. #2286 and #2288 both bumped `zenoh-pico` forward and
merged while this job was queued. The job compared against that newer main.
Any job that runs `check fast` on a push to main has this exposure, not only
`check / fast`.

## Resolution (2026-10-18)

The baseline is now a property of the COMMIT UNDER TEST, decided in one place:
`scripts/ci/pin-baseline.sh`.

| event | baseline |
| --- | --- |
| `push` | `github.event.before`, the commit the push replaced (all-zero: the tested commit's first parent) |
| `merge_group` | `merge_group.base_sha` |
| `pull_request` | `pull_request.base.sha`, fixed in the payload |
| `schedule`, `workflow_dispatch` | the tested commit's first parent, read from the raw commit object so a depth-1 clone can name it |
| local (`just check fast`) | `git merge-base HEAD origin/main`, the branch point |

An older baseline cannot produce a false rewind: a forward move it has not
seen yet still reads as a forward move. Only a newer baseline can, and the live
tip is the newest there is. The local default had the same defect as the push
lane: a branch that main had moved past inherited every later pin bump on main
as a REWIND of its own. The pre-push hook was already correct, because git hands
it the remote ref's sha, which is the push's `before`.

**One spelling.** Nine copies of the three steps (eight jobs in `gate.yml`, one
in `host-tests.yml`) each spelled `origin/${GITHUB_BASE_REF:-main}` inline. They
now call `pin-baseline.sh provide` (resolve, fetch that commit with depth 1,
export `NROS_SUBMODULE_PIN_BASELINE`), `init-moved` and `deepen`. The second and
third used to diff against the live tip too, so they could initialise and
deepen against the wrong pins. `submodule-pins-check.sh` sources the same
`nros_pin_baseline_resolve` when it is given no baseline, so the workflow and
the gate cannot disagree about the rule. If no baseline can be named, the gate
does not fall back to `origin/main`. It reports an unresolvable baseline:
FATAL in CI, NOT CHECKED locally. Issue 1043's three outcomes and issue 1476's
shallow rule are unchanged, because both apply after the baseline is chosen.

**Gate.** The script's normal-path selftest builds its own superproject (issue
1465): `S0` pins `sub@old`, `S1` moves no pin, `S2` bumps to `sub@new` and is
`origin/main`, and `S3` pins `sub@old` again. Checking `S1` against the live tip
is the control and must say REWIND, which proves the fixture reproduces the
race. Every arm (local, push, merge_group, pull_request, dispatch) must then
call `S1` clean, and every arm must still FAIL `S3`. A payload with no base
must fail as unresolved, not as a REWIND. The selftest adds about 0.1 s.

Mutation proof (each mutation reverted, restored with `cmp`, back to rc 0):

| mutation | gate |
| --- | --- |
| local arm returns `origin/main` instead of the merge-base | rc 1, "local default (merge-base) must not call S1 a REWIND" |
| push arm ignores `before` | rc 1, "push event (before=S0) ..." |
| merge_group reads `head_sha` | rc 1, "merge_group (base_sha=S0) ..." |
| pull_request reads `head.sha` | rc 1, "pull_request (base.sha=S0) ..." |
| dispatch arm returns the tested commit, not its parent | rc 1, "dispatch (first parent) ..." |
| an unresolvable baseline falls back to `origin/main` | rc 1, "merge_group with no base_sha must fail as UNRESOLVED, not REWIND" |
| the gate ignores the resolver (the old default) | rc 1, "local default ..." |
| `pin-baseline.sh` is absent | rc 2, "cannot load scripts/ci/pin-baseline.sh" |

A "clean" case must print `submodule-pins: OK`. An unresolvable baseline also
exits 0 locally (NOT CHECKED), and the selftest must not count that as clean.
The first draft of the selftest did count it, and the hook-side-effects gate's
checkout, which lacked the then-untracked script, showed the hole.

**Symptom, on the first sighting's real history** (`d4ed5af76`, superseded by
`eadf501a4c`'s forward `third-party/nuttx/libc` bump):

- old baseline, live main `eadf501a4c`: `REWIND`, rc 1, which is the CI failure above
- push arm, `before` = `d4ed5af76^`: `OK (0 pin(s) moved)`, rc 0
- a real rewind (a push whose `before` is `eadf501a4c` landing the older libc pin): `REWIND`, rc 1

Not measured: a live CI run of the new step. `workflow_dispatch` reaches it
through the first-parent arm (`gh workflow run gate.yml --ref <branch>`). The
push, merge_group and pull_request arms are covered only by the selftest's
synthetic payloads.

