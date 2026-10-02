---
id: 1345
title: "Two gates report a hard FAIL for \"I have no tree to look at\", which is
  the DEFAULT state in an agent worktree — so `ci gate` stops at step 2 and four
  later steps are withdrawn, having examined nothing"
status: open
type: bug
area: ci, tooling
severity: medium
found: 2026-09-12
related: [issue-0952, issue-1043, issue-1280, phase-454]
---

## Measured

In a linked agent worktree, `just ci gate` stopped at **step 2 of 6** on two
gates, neither of which had examined the commit under test:

```
check-capability-conditionals: zenoh-pico/include/zenoh-pico/system/common/platform.h
  is missing … The socket-ABI rule cannot be checked without it.
check-xrce-vendored-versions: no vendored tree checked out — nothing verified.
```

Both messages say plainly that **nothing was verified**. Both exit non-zero
anyway.

The cause is not a defect in the submodules: `zenoh-pico`, `micro-cdr` and
`micro-xrce-dds-client` were empty directories, their object stores already
present in `.git/modules`. `git submodule update --init` on the three checked
them out at their recorded pins **with no fetch**, and both gates then passed:

```
check-capability-conditionals: OK (5 platform manifest(s), 5 capability-gated row(s), IP_STACK = 'ip_stack')
check-xrce-vendored-versions: OK — micro-cdr 2.0.2, micro-xrce-dds-client 3.0.1
```

An agent worktree does not init submodules by default, so **"no tree to look at"
is the ordinary condition there, not an exception**.

## The distinction these gates do not make

`check-submodule-pins` already models this correctly and issue 1043 is the
write-up: it has **three** outcomes, not two — FAIL (ancestry measured, not a
fast-forward), **NOT VERIFIED** (no object store here — a reported skip via the
`nros_check_skip` ledger, because no lane checks out all 20 submodules), and OK.

The mechanism is in the tree and other gates use it (`scripts/build/check-skip.sh`;
`check-cxx-compat-shim-facilities.py`, `check-ivc-fsp-compile.sh` among its
consumers). Neither gate here references it:

| gate | `nros_check_skip` references |
| --- | --- |
| `scripts/check-capability-conditionals.py` | **0** |
| `scripts/check-xrce-vendored-versions.py` | **0** |

So each collapses "I could not look" into "I looked and it is wrong", which are
different claims with different remedies.

## Why the cost is larger than two gates

`ci gate` stops at the first failing step. A FAIL at step 2 **withdraws
`check::build`, `check::api-parity`, `test-unit` and `test-lane-contracts`** —
four steps that never ran, over a commit neither failing gate had read.

That is issue 0952's shape, and the lane's own footer warns about it. The
practical effect during phase-454: **five consecutive waves could not obtain a
local `check::build` verdict**, each spent time triaging reds that were
environmental, and each fell back to CI for the answer. Combined with issue 1280
(inherited paths making a worktree build measure the wrong tree), the compile
tier has been effectively unavailable to parallel agent sessions.

A red lane that is red for a reason unrelated to the change also has **no signal
capacity** — the class CLAUDE.md describes: a regression landing in it looks
exactly like yesterday's failure.

## What a fix has to decide

* Route both gates through the existing skip ledger so an absent vendored tree is
  **NOT VERIFIED**, reported, and non-fatal — matching `check-submodule-pins`.
* Decide whether the ledger's report should be loud at the END of the lane rather
  than inline, so a lane that skipped half its gates cannot read as a clean green.
  `check-submodule-pins` has `NROS_SUBMODULE_PINS_STRICT=1` for a lane that really
  does provide every submodule; the same escape hatch applies here, and issue 1043
  records that setting it on a lane providing a subset **re-creates the bug**.
* Sweep rather than fix the two reported sites. The rule is "a gate that cannot
  reach its subject reports NOT VERIFIED", and these two were found by tripping
  over them — issue 0196's shape says there will be more. Enumerate every gate
  that reads a vendored tree.

## Not the same as issue 1280

1280 is *inherited absolute paths make a worktree build the wrong checkout*. This
is *an absent vendored tree is reported as a defect*. They compound — a worktree
hits both — but the fixes are independent, and 1280's fix does not address this.

## Reproduction

In any linked agent worktree with uninitialised submodules, run `just ci gate`.
It stops at step 2 with the two messages above. `git submodule update --init
zenoh-pico micro-cdr micro-xrce-dds-client` (no fetch needed — the object stores
are already in `.git/modules`) makes both pass.

Found during phase-454 W8.

## The same two gates, in a venue this issue did not name: the HOSTED push lane

Measured 2026-09-20. `gate.yml`'s **push** lane on `main` fails on exactly this
pair, and has done so on every completed push run for at least three days:

```
run 35505068333  push gate  main @ 1785cdc6  2026-09-20T10:26:17Z  failure
  job 106063395162  check (fast on push; full on PR/nightly)
    step `just check fast`
      ===== FAIL (capability-conditionals, rc=2, 972ms) =====
      ===== FAIL (xrce-vendored-versions, rc=1, 1029ms) =====
      check-fast (parallel): 2 of 327 gate(s) FAILED
```

The messages are the ones above, verbatim, including their advice to run `just
setup-worktree` — on a GitHub runner, where there is no worktree to set up.

**The runner is not a worktree, and it is not misconfigured either.** The job's
own step list says why: `Provision compile-tier sources` is **skipped** on a
push event (it is gated to the compiling events), so the three vendored trees
are absent by design on this lane, exactly as they are absent by default in an
agent worktree. Two different environments, one rule: nothing provisioned the
subject, and the gates call that a defect.

**It is uniformly red, which is this issue's "no signal capacity" argument
already happening.** Every completed `event=push`, `branch=main` run of
`gate.yml` that the API returns is a failure:

| run | created | head |
| --- | --- | --- |
| 35505068333 | 2026-09-20T10:26 | 1785cdc6 |
| 35311024202 | 2026-09-18T05:29 | 3a3ec205 |
| 35304501105 | 2026-09-18T03:47 | 5ddee305 |
| 35301863424 | 2026-09-18T03:05 | b3316b38 |
| 35291099863 | 2026-09-18T00:25 | 156a7ea1 |
| 35289417069 | 2026-09-18T00:01 | 772b21e3 |
| 35286479168 | 2026-09-17T23:21 | aebe59b2 |
| 35282943532 | 2026-09-17T22:37 | 520db43a |

Spot-checked rather than assumed: 35311024202's job 105492954955 prints the same
three lines, `2 of 327 gate(s) FAILED` on the same two names. So a real
`check fast` regression landing on `main` would arrive in a lane that has been
red for every push since 2026-09-17 and would look exactly like it.

Two consequences worth stating, because neither is obvious from the run list:

- **Nothing is blocked by this.** The required context for a pull request is the
  aggregator `CI` on the `pull_request` and `merge_group` events, where
  `Provision compile-tier sources` does run; the push lane is advisory. That is
  why it has been red for days without anyone's PR noticing.
- **The withdrawal cost applies here too.** After `just check fast` fails, the
  job's remaining steps — `check submodule-commits-reachable`, the fixtures, the
  generated bindings, `test-unit`, `test-lane-contracts`, `check workspace-all` —
  are all `skipped`. A lane that examined 325 of 327 gates and then abandoned the
  rest reports as one word: `failure`.

This does not change what a fix has to decide; it widens the sweep. Routing both
gates through the skip ledger fixes the worktree case and this one at once,
whereas provisioning the submodules on the push lane would fix only this one and
leave the agent worktrees where they are.

## Before copying the idiom: one of the two ledger mechanisms does nothing (2026-09-22)

The fix above says to route both gates "through the existing skip ledger", and
the table names `check-cxx-compat-shim-facilities.py` as a consumer. Measured
before reusing it: **there are two different mechanisms, and the Python one is
inert.**

* The SHELL ledger works. `scripts/build/check-skip.sh`'s `nros_check_skip`
  appends to `$(nros_build_dir "$NROS_KIND_CHECK_SKIPS")/checks.skipped`, and
  `just/check.just:201` ends the fast lane with
  `nros_check_skip_report "Fast checks passed!"`, which is where the
  `[SKIPPED] …` lines in a local `check fast` summary come from.
* The PYTHON spelling reads an environment variable **nothing sets**:

  ```
  $ grep -rn NROS_CHECK_SKIP_LEDGER .            # excluding .git
  scripts/check-cpp-freestanding-mechanisms.py:133:  ledger = os.environ.get("NROS_CHECK_SKIP_LEDGER")
  scripts/check-cxx-compat-shim-facilities.py:280:  skip_ledger = os.environ.get("NROS_CHECK_SKIP_LEDGER")
  ```

  Two readers, zero writers, in the whole tree. Both guard the write with
  `if skip_ledger:`, so with the variable unset the branch is skipped silently
  and the skip is never recorded — it prints a `notes` line inside the gate's
  own output and reaches no summary.

So `check-cxx-compat-shim-facilities.py`'s docstring — "reported as a SKIP
through the `nros_check_skip` ledger rather than passing quietly, because a skip
that reads like a pass is how the gap being fixed here survived" — describes a
behaviour the code does not have. That is this issue's own shape one level in:
a claim about reporting that nothing checks.

Consequence for the fix: routing `check-capability-conditionals.py` and
`check-xrce-vendored-versions.py` through the Python idiom as it stands would
make them non-fatal AND unreported, which is strictly worse than today's hard
fail. Whatever lands has to either set `NROS_CHECK_SKIP_LEDGER` from the recipe
(one producer, gated), or give the Python gates an exit code the recipe
translates into a `nros_check_skip` call — the shell ledger being the one that
demonstrably reaches the summary. The two existing readers should move to the
same spelling in that sweep; they are part of it, not a precedent for it.

## It is not only agent worktrees — main's push `gate` lane has this on every push (2026-10-02)

The entry above frames "no tree to look at" as the ordinary condition **in a
linked agent worktree**. It is also the ordinary condition on the `push` event
in CI, which this issue did not record, and there it costs a lane on `main`.

Every completed push `gate` run on `main` on 2026-10-01 failed, five of five,
with the same two gates and the same count:

```
2 of 379 gate(s) FAILED
check-capability-conditionals: packages/…/zenoh-pico/include/… is missing
check-xrce-vendored-versions: no vendored tree checked out — nothing verified.
```

Runs 36938229122 (`2eb977f6f`), 36933780738 (`0183a0434`), 36926328875
(`d84a808b8`), 36920672159 (`d986c270a`), 36867095678 (`85ae1d156`) — job
`check (fast + PR source gates; …)`, step 14 `just check fast`. The second
string is verbatim the one measured in the worktree above, so the mechanism is
identical; only the host differs.

### Why the push lane has no tree, and why the PR lane does

`gate.yml`'s provisioning step is conditioned on the event, and `push` is not in
the list:

```yaml
- name: Provision compile-tier sources
  if: ${{ contains(fromJSON('["pull_request","merge_group","schedule","workflow_dispatch"]'), github.event_name) }}
  run: |
    nros setup --source px4-rs --source zenoh-pico --source mbedtls \
      --source micro-cdr --source micro-xrce-dds-client --source cyclonedds-src
```

Measured on both sides of that condition, same job, same gate list:

| event | `Provision compile-tier sources` | `just check fast` |
| --- | --- | --- |
| `pull_request` (36941574537, 36941816207) | success | **success** |
| `push` (the five runs above) | **skipped** | **failure** |

Step 12, `Init submodules whose pin moved (commits only)`, does run on push —
but it fetches commits for `check-submodule-pins`, not working trees, so it
cannot satisfy either gate.

### What this makes it

The comment immediately below that step in `gate.yml` states the contract these
two gates break:

> check-fast is buildless + source-free: the C/C++ gates here are clang-format
> only … The compile gates … (which pull the ros-launch-resolve submodule + the
> zenoh-pico source) live in [the compile tier]

So the lane's own authored contract says `check fast` needs no sources, and two
of its 379 gates need vendored sources. That is the `check-lane-contracts` rule
— a gate in an affordability tier may only resolve artifacts the job itself
provides — one event over from where that gate looks.

The consequence is the one CLAUDE.md names: **a lane red every cycle has no
signal capacity.** A genuine `check fast` regression landing on `main` today
would have arrived as `2 of 379 gate(s) FAILED` beside two gates that examined
nothing, indistinguishable from yesterday's failure, on every push.

### What this is NOT

It is not a merge-gating failure and nothing broken lands because of it. The
required `CI` context on a pull request is produced on the `pull_request` event,
where the step runs and `check fast` passes; the merge queue runs on
`merge_group`, which is also in the list. This is why it survived a whole day
unremarked — the lane that is uniformly red is the one no one is required to
read.

It is also not 1226. There the gate ran nowhere; here it runs, on an event where
it cannot answer.

### What would close this half

The three-outcome treatment this issue already prescribes — a reported
NOT VERIFIED through the `nros_check_skip` ledger when the tree is absent,
rather than a hard FAIL — fixes the push lane and the agent worktree with one
change, and leaves the gates fully load-bearing on the three events that
provision. Adding `push` to the step's event list would also turn the lane
green, but it buys a provisioning run on every push for two gates, and it
leaves every agent worktree exactly where this issue found it.
