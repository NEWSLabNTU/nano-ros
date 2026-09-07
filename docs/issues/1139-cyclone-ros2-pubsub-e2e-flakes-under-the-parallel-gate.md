---
id: 1139
title: "`nros_rmw_cyclonedds_ros2_pubsub_e2e` fails under the 21-way parallel
  gate and passes solo, every time -- 6 failures, 4 solo passes, one session"
status: open
type: bug
area: testing, rmw-cyclonedds
severity: medium
related: [issue-1009, issue-0741]
---

## What happens

`just ci gate` -> `check::build` -> `rmw-cyclonedds` fails on test 20,
`nros_rmw_cyclonedds_ros2_pubsub_e2e`. Re-running the SAME test alone in the
same build dir passes, every time it has been tried.

Measured over one session, 2026-09-05/06, on a tree whose diff contains no
CycloneDDS code at all:

| run | in-gate | which sub-case failed | solo re-run |
| --- | --- | --- | --- |
| 1 | FAIL | A.2 `ros2 pub -> nros sub` timed out | pass |
| 2 | FAIL | A.1 `nros pub -> ros2 echo` captured nothing | pass |
| 3 | pass | -- | -- |
| 4 | FAIL | A.1 | -- |
| 5 | pass | -- | -- |
| 6 | pass | -- | -- |
| 7 | FAIL | A.1 | pass |
| 8 | FAIL | A.1 | pass |

The failing sub-case is not fixed: A.1 and A.2 have each failed, and each has
passed in the same conditions. That is the signature of a timing or contention
problem rather than a broken assertion -- a broken direction would fail the
same way every time.

Wall-clock also varies wildly for the same lane: 35 s, 155 s, 202 s, 220 s,
316 s across the runs above. The 35 s one failed early rather than timing out.

## Why it matters even though it "passes solo"

A lane that is red for a reason nobody has established has no signal capacity.
This repo already records that lesson (`just nightly-triage`, and the note in
CLAUDE.md that "a red CI lane answers one of two questions and they look
identical"). Right now a real CycloneDDS regression landing in this cell would
look exactly like the eighth flake, and the honest response -- rerun it solo --
is also exactly what would hide the regression.

It also costs a full `just ci gate` cycle every time it fires, because the lane
stops at the first failure and withdraws every step after it.

## What has NOT been established

* Whether the contention is CPU (the gate runs `-P20` and this cell starts a
  real `ros2` process pair), the DDS bus, or the discovery timeout.
* Whether it predates the parallel gate's current width.
* Whether the two sub-cases share a cause or fail for different ones.

Nothing here has been bisected. The table above is observation, not diagnosis.

## Where to look first

Issue 1009 is the near neighbour and worth reading before touching this: the
interop DDS bus is pinned to loopback by a PROFILE FILE the harness writes
(`nros_tests::dds_isolation`), and getting that half-right isolates one side of
a pair and measures zero with empty output. If this cell's isolation is
partial, a busy machine is exactly when a foreign participant would win a
discovery race.

`ROS_LOCALHOST_ONLY=1` is NOT the fix and is documented as making it worse.

## Workaround in the meantime

Re-run the lane; it passes about a third of the time. Do NOT read a solo pass
as proof the diff is clean -- it is proof of nothing about the diff, only that
the cell is not deterministically broken.

## 2026-09-07 -- two mechanisms REMOVED, the flake itself NOT REPRODUCED

Read the second half of that heading before the first. What follows removes two
measured defects from this cell. Neither has been shown to be the cause of the
eight runs tabled above, because **the flake did not reproduce here at all**,
and this issue therefore stays OPEN.

### What did not reproduce

Every attempt used the binaries built from this cell's own sources, unchanged.

| condition | runs | failures |
| --- | ---: | ---: |
| solo, host otherwise busy (load avg ~14) | 1 | 0 |
| 30 spinner processes on 20 cores | 8 | 0 |
| 4 concurrent copies of the cell, 5 rounds | 20 | 0 |
| 6 concurrent copies, 60 spinners, load avg 64 | 18 | 0 |

47 runs, 0 failures. Two of those 18 concurrent copies drew the SAME domain
(64) and both still passed, so a same-domain collision between two copies is
not sufficient either -- both copies publish the same payload, which is exactly
why 0580's cross-talk is invisible in this direction.

`ros2 topic echo` was also timed directly against an already-publishing talker:
0.85 s / 0.94 s / 0.85 s idle, and 0.85 s / 0.83 s / 1.29 s under 60 spinners
at three times oversubscription. CPU contention on this host does not move the
Python CLI's startup enough to matter, so "the gate is busy" is not on its own
an explanation, and the third open question above ("whether the contention is
CPU") has a partial answer: not CPU alone, not here.

What was NOT tried, and is the obvious next step for whoever picks this up: a
real `just ci gate`, whose 20 concurrent gates each run their own
`cmake --build --parallel` and `cargo nextest`. That is a different load shape
from spinners -- memory bandwidth and page cache, not runnable threads -- and
it is the only condition the original table was collected under.

### Mechanism 1: this cell's bus was on the LAN. Measured.

The "where to look first" note above is right, and understated. Both shell
cells here (`ros2_pubsub_e2e.sh`, `ros2_srv_e2e.sh`) each wrote their OWN
CycloneDDS config pinning the bus to a real ethernet interface with
`multicast="default"`. Every other DDS lane in the tree has been confined to
loopback since issue 1009; these two were exempt for one reason, that they are
shell and `nros_tests::dds_isolation` is Rust. `check-dds-isolation-symmetry`
could not see them either -- it scans `packages/testing/nros-tests/**/*.rs`.

Measured on this host, domain 96, with a foreign `/chatter` publisher started
on the OLD config (payload `hello-from-alien`) and the nano-ros subscriber
binary run against it five times per config:

| subscriber config | foreign sample taken |
| --- | ---: |
| old (ethernet interface, multicast on) | **5 / 5** |
| new (loopback, `AllowMulticast=false` + localhost peer) | **0 / 5** |

Negative control, because a config that still delivers proves nothing -- an
inert one delivers too. With the loopback address moved to `10.255.255.254`
the cell FAILS, and now says why: `10.255.255.254: does not match an available
interface`, publisher rc=2, twenty attempts. The file is read.

A same-host publisher pinned to the ethernet interface is a PROXY for a peer on
another host, not the thing itself; it reaches us by the same multicast SPDP a
real LAN peer would use. That the pin defeats a genuinely remote participant is
inherited from 1009's batch G and is not re-measured here.

### Mechanism 2: every timing constant was a bet on a Python CLI. Measured.

A.1 gave `ros2 topic echo` a 1 s head start and an 8 s window against a
publisher that lived about 7 s, so the whole sub-case turned on the CLI being
up inside roughly five seconds. A.2's subscriber has a 10 s budget compiled
into `ros2_sub.cpp` and got one attempt.

`ROS_SETUP` was pointed at a wrapper that puts a shim `ros2` on PATH, adding a
fixed delay before exec'ing the real one -- a controlled stand-in for a host
where the CLI is slow to start. Two sweeps, 5 runs then 3, eight per cell:

| added `ros2` startup delay | old script | new script |
| --- | ---: | ---: |
| 0 s | 8 pass / 0 fail | 8 pass / 0 fail |
| 3 s | 6 pass / **2 fail** (both A.1) | 8 pass / 0 fail |
| 6 s | 0 pass / **8 fail** (8 A.1, 1 A.2) | 8 pass / 0 fail |
| 10 s | 0 pass / **8 fail** (8 A.1, 8 A.2) | 8 pass / 0 fail |

The 3 s row is the interesting one: the old script fails INTERMITTENTLY --
2 of 5 in the first sweep, 0 of 3 in the second -- and A.1 is the sub-case that
goes first. That is the shape this issue tabled: A.1 dominant, A.2 rarer, both
directions passing and failing in the same conditions. It is a matching
signature, not a proof of cause: nothing shows the gate actually delays the CLI
by seconds.

### Mechanism 2b: one leaked `ros2` daemon per run, and they last hours

Not a failure mode of a single run, but it is what a gate accumulates, and it
was found by reading `ps` during the runs above. `ros2 topic echo` without
`--no-daemon` asks `NodeStrategy` for a daemon; the `ros2 daemon stop` at the
top of the script guarantees there is none, so every run SPAWNS one, on that
run's domain, and it outlives the test by about two hours.

| script | 5 runs | daemons leaked |
| --- | ---: | ---: |
| old | 5 | **5** |
| new (`--no-daemon` on `topic echo`) | 5 | **0** |

Collecting the tables above left **68** orphaned `ros2-daemon` processes on this
host, one per domain visited, each holding a CycloneDDS participant announcing
on the ethernet interface the old config pinned. Whether that background
discovery traffic is enough to matter is NOT established -- but it is the same
mechanism issue 1009 direction 2 fixed in `dds_bus_snapshot`, and it is the
reason a long gate run's later cells face a busier bus than its first.
(`ros2 topic pub` needs no flag and rejects one: it builds a DirectNode.)

### Mechanism 3, not a failure mode but why nobody could bisect one

The publisher's stdout AND stderr went to `/dev/null`, its exit status was
discarded by a bare `wait`, and `ros2`'s stderr went to `/dev/null` too. A
publisher that never opened a session and a delivery that never happened
printed the identical line, `captured 0 line(s)`. That is the whole of why this
issue says "nothing here has been bisected".

The evidence is kept now and dumped on failure. It paid for itself on its first
run: `ros2 topic pub` does not accept `--no-daemon` in Humble, which the new
script had grown alongside `topic echo`, and the log named it in one line
instead of six identical timeouts.

### What landed

* `nros_export_cyclone_config` in `ros2_e2e_common.sh` -- ONE spelling of the
  bus config for both scripts, mirroring `dds_isolation.rs`'s
  `CYCLONE_LOOPBACK_XML`. Symmetric by construction: `CYCLONEDDS_URI` is
  exported, so the `ros2` peer and our binaries read the same file (issue 1137
  is what half a pin costs). `NROS_DDS_ALLOW_LAN=1` opts out; an operator's own
  `CYCLONEDDS_URI` is no longer overwritten.
* `ros2_pubsub_e2e.sh` waits on a DEADLINE and stops the moment the payload
  lands. The happy path got faster, not slower -- about 3 s where the fixed
  windows cost 12 s.
* Retries that cannot launder a verdict: A.2 retries a subscriber that took NO
  sample, and never one that took the WRONG sample. A wrong payload is a
  foreign publisher or a codec fault and fails immediately.
* `check-dds-isolation-symmetry` grew a shell arm, so a future shell cell
  cannot write its own inline `<CycloneDDS>` config. Issue 0196's rule: the
  gate's reach was narrower than the rule it enforced.

### Acceptance, still unmet

`just ci gate` green with `nros_rmw_cyclonedds_ros2_pubsub_e2e` passing across
enough consecutive runs to beat the tabled ~1-in-3 failure rate. That needs the
real gate, which this work did not run.
