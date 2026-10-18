---
id: 1956
title: "A FreeRTOS island under flood re-dispatched ONE buffered subscription sample ~289k times and never ran another callback"
status: open
type: bug
area: [executor, rmw-zenoh, freertos]
severity: high
found: 2026-10-18
related: [1948, 506, 1940, phase-462]
---

## Measured

nano-ros-rt-eval, FreeRTOS mps2-an385, QEMU 11.0.0-nros6, ~2 kHz UDP flood
(`--load-count 4`), image built with `NROS_ZENOH_RX_TASK_BUDGET_FRAMES=8`
on nano-ros main `7bbead7846`. Run `results/20261018-091453Z` in
NEWSLabNTU/nano-ros-rt-eval `9f17c74`, beside the 11 clean runs of the same
check (`results/w6a_flood_cells_2026-10-18.md` §3).

2.4 s into the nominal phase, the actuator's `on_cmd` subscription callback
began running back to back on the SAME sample:

```
[INFO] nros: [ctrl] tick=530 v=0.999 a=0.002 t_us=7334079
[INFO] nros: [e2e] seq=78
[INFO] nros: [e2e] seq=78
[INFO] nros: [e2e] seq=78
...
```

- 288,787 `[e2e] seq=78` lines in ~28 s; island.csv 12.7 MB (a normal run
  is ~6,800 lines).
- Nothing else ran after it: `[ctrl]` stops at tick 530 (a normal run
  reaches ~3,300), and no `[rx1s]`, `[vel]`, `[wd]` line follows.
- `[e2e]` is logged only for the peer's commands (`stamp.sec != 0`); flood
  samples carry `stamp.sec = 0` and print nothing. So the log cannot tell
  "one sample re-dispatched" from "one TCP batch re-parsed with its flood
  frames between" — only that the peer's seq 78 is delivered over and over
  and no later peer sample (seq 79+) ever is, while the peer process ran
  to the end of the run (its own heartbeat reached seq 100).
- No WARN, fault or assertion before it; boot log is identical to the
  clean runs (same subscription grants, same stack peaks).

## Rate

1 of 12 runs on `7bbead7846` (1 of 6 `budget8`, 0 of 6 `unbounded`), with a
`just ci gate` loading the host during all of them. 0 in ~20 flood runs of
the same images built on `6486216ab7` plus the then-local 1943/1948 patches
(phase-462 W6a, 2026-10-18), and 0 in the ~210 runs the eval workspace has
recorded since 2026-08. Too few to call it a regression in the 51 commits
between the two pins, and too rare to bisect at this rate.

8 further `budget8` runs on an idle host (eval `results/20261018-0927*`
to `-0932*`, not committed) had no runaway; total 1 of 14 `budget8`.

### A second, quieter shape — and it is OLDER than this pin

One of those 8 (`20261018-093104Z`) went SILENT instead: last `[ctrl]` at
island 24.9 s, then no output of any kind for the final 10.1 s, no
runaway, no WARN. The stall counter the eval prints misses this, because it
measures gaps BETWEEN `[ctrl]` lines and there is no line after the last.
Scanning every recorded run for a silent tail over 0.5 s finds it well
before `7bbead7846`:

| run | label | silent tail |
|---|---|---|
| 20260808-034242Z | freertos-baseline-load | 2.6 s |
| 20260810-165133Z / 165210Z | wnd8-2khz-r1 / r2 | 12.8 s |
| 20260821-032029Z | budget8-r2 | 2.9 s |
| 20260821-032257Z | budget32-r3 | 2.9 s |
| 20260821-032659Z | unbounded-r5 | 12.9 s |
| 20260821-054931Z | budget128-r7 | 12.9 s |
| 20260821-055008Z / 055045Z | unbounded-r8 / budget8-r8 | 2.9 / 3.0 s |

(The 12.9 s ones end exactly one phase early, so some may be the driver
tearing the island down rather than a hang; the 2.9 s ones are mid-phase.)
None of the 12 phase-462 W6a gate runs has one. Whether the runaway and
the silence are one defect is open; a runaway whose sample does not log
(a flood sample, `stamp.sec = 0`) would look exactly like a silence.

## Candidates (code read at `7bbead7846`, not yet tested on target)

The actuator's subscription takes the IN-PLACE path: `BufferedRaw` is
in-place capable and the zenoh backend says so through the cffi adapter, so
it is a `SubInplaceRawEntry` reading the shim's per-subscription ring
(`SubscriberBuffer`, depth 4) directly. The C read task is that ring's only
producer (`_zpico_ring_push`, `zpico.c`, Release-store of `ring_tail`); the
executor is its consumer (`peek_head_slot` + `consume_head`).

1. **The in-place drain has no bound** — `executor/arena.rs`
   `sub_inplace_raw_try_process` loops `process_raw_in_place` until it
   returns `false`, i.e. until the ring is empty AT THE CHECK, while a
   higher-priority read task refills it. One `try_process` can then never
   return, and no other entry on that executor (or any lower tier on the one
   core) runs again. The buffered paths are bounded by what was copied in.
   Alone this would dispatch FRESH samples, so it explains "nothing else
   runs" but not seq 78 repeating — unless combined with (2). Predates the
   range (#1340).
2. **`ring_head` overtakes `ring_tail`** — then `has_data` (`head != tail`)
   is true forever, `peek_head_slot` cycles `head % 4` over the same <= 4
   stale slots, and the producer's `tail - head >= slot_count` underflows
   to "full" and drops every new sample. With (1) that is exactly the
   symptom: a small fixed set of samples (one peer sample, the rest
   unlogged flood samples) dispatched forever and nothing new arriving.
   `consume_head` is a plain load-then-store, so it only stays sound with
   ONE consumer per buffer. One way to get two: `NEXT_BUFFER_INDEX` hands
   out buffer indices with `fetch_add` and rolls back with `fetch_sub` on
   three failure paths in `ZenohSubscriber::new`, so a failed create racing
   another tier's create can let the next create reuse a live index — two
   subscriptions, two producers, two consumers on one ring. Reachable only
   if some create fails on the island, which nothing in the log shows yet.
3. **Less likely: `entity_wake.rs` spin lock** (#1940, `4f8f9a73f1`) taken
   by the read task while a lower-priority task holds it. Nothing on the
   `ExecutableNode` path registers a waker, and the symptom would be RX
   dying rather than one callback looping.

No commit in `6486216ab7..7bbead7846` touches the ring, the take, or the
in-place dispatch, so (1)/(2) would mean a latent defect the flood plus
the RX budget exposes, not a regression in the range.

## Next

Tell (1) from (2) on the stuck island: dump the stuck buffer's
`ring_head` / `ring_tail` through the QEMU gdbstub (`gdbprobe.sh` from the
1948 diagnosis). `head` past `tail` (wrapping) is (2); `0 < tail - head <=
4` with the read task still advancing `tail` is (1). Bounding the in-place
drain (at the ring depth, or at the count seen on entry) is the fix for (1)
either way.
