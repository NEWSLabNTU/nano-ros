---
id: 2777
title: "A threadx_linux image that outlasts a late router then exits: the stock parameter services ask for 283,392 bytes in one allocation (a regression from 6229085857)"
status: resolved
type: bug
area: [threadx, zenoh, ci]
severity: high
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "fix(issue 2777): a migrated cargo leaf's stock parameter services imply a store"
related: [2207, 2760, 2624, 2706, 2780, phase-556, issue-2772]
---

## What fails

`rtos_e2e::test_rtos_session_open_outlasts_a_late_router::platform_1_Platform__ThreadxLinux`
in `merge-runtime.yml`, lane `merge`, shard `threadx_linux`:

> threadx_linux late router: the image started 3s before its router and never
> reported `Application setup complete` — an image built with connect_forever
> did not outlast the gap, so its open is not retrying (issue 2207).

The listener prints its banner, the network bring-up and `[INFO] nros: boot`,
and nothing after: it is stuck in, or before, the session open. No error is
printed. On a board this is an image that boots and silently never joins the
graph whenever its router comes up second.

## A regression, not a flake

| run | main | date | this test |
| --- | --- | --- | --- |
| 44030478812 | cf3589e386 | 2026-11-17 | PASS |
| 44090176782 | 2cfe9f0094 | 2026-11-17 | PASS |
| 44721090137 | 0d9429d990 | 2026-11-20 | FAIL |
| 44768834808 | b13a5d4b18 | 2026-11-20 | FAIL (the shard's only real failure) |

Read from each run's `merge-runtime-merge-threadx_linux-junit` artifact.

## Where to look

`2cfe9f0094..0d9429d990` is 451 commits. By subject, most likely first:

1. `1a896a6722` fix(smoltcp, zenoh): durations read the monotonic clock
   (issue 2760). The open's retry sleep and its connect budget are durations.
2. `2a0260282f` the ThreadX byte pool is derived from measured working sets
   (issue 2624). An open that retries allocates per attempt and could exhaust
   a tighter pool without saying so.
3. `8cd0dfd9a7` an interrupt wakes a parked ThreadX executor (issue 2706).
4. `6229085857` every image serves the stock node interfaces by default: more
   entities to declare once the session is open.

Not bisected: threadx_linux needs the veth bridge, which the host that found
this cannot set up.

## Closes when

The test passes again on the merge lane, with the cause named here.

## Bisected, 2026-11-21 (nros 142)

The first failing commit is `6229085857` (phase W13: every image serves the
stock node interfaces by default). Its parent `ed72edbd8f` passes 3 of 3.

The open is NOT what fails. The session opens once the late router appears;
after that, the parameter services cannot be created and the image exits 1:

```
parameter store refused: needs 283392 bytes in one allocation (32 slots x 8856)
Application error: Capability { name: "param_services", reason: "Transport::BadAlloc" }
```

So the title's first reading ("its open is not retrying", the test's own
message) was wrong. The test's message is phrased for the one failure it was
written for: a missing `Application setup complete` line. Any failure after
the open reaches it the same way. The real defect is a pool size: the
default-on stock interfaces add a parameter store whose single allocation the
ThreadX byte pool cannot satisfy, which suspect 2 above (`2a0260282f`,
pools derived from measured working sets) may have tightened.

The fix is with the nros 175 session. Closes when the test passes again on
the merge lane.

## Fix (2026-11-21, nros 175)

The sizing descriptor of `examples/threadx-linux/rust/listener` said
`[params] store = "none"` beside `infra_families = "param+rosout+clock"`. The
cargo-leaf store view keeps the `param_services` axis only where the build
compiles the services in, and it looked for the `param-services` feature in
the leaf's own Cargo.toml. A migrated leaf names none: its generated selection
package carries the `nros` features, and those are the image's effective axes.
So `nros-params` carved no store (`IMPLIED_STORE_SLOTS = 0`), and the runtime
asked the heap for the crate default.

- `sizing_descriptor::leaf_compiles_param_services` counts a leaf that depends
  on its selection package. The descriptor now states
  `store = "param_services"`, and 8 slots are carved.
- `nros::main!` const-asserts that an image serving the parameter services
  carves a store, naming the knob, so a road that loses the fact fails at
  build instead of at boot (phase-556 W13 refinement).

`rtos_e2e test_rtos_session_open_outlasts_a_late_router` (ThreadxLinux)
passes 3/3.
