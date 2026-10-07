---
id: 1709
title: "A zenoh transient-local publisher retains ONE sample whatever it declares —
  `TL_RETAIN_DEPTH` is a constant, not a derived knob"
status: resolved
type: limitation
area: [rmw]
severity: low
found: 2026-10-06
related: [1687, 1740, phase-480]
---

## What is true today

`nros-rmw-zenoh/build.rs` emits `pub const TL_RETAIN_DEPTH: u32 = 1;`, and
`shim/qos.rs::admit` grants a transient-local publisher at most that depth and
advertises the grant in the liveliness token (phase-455 W5). So a publisher that
declares `TRANSIENT_LOCAL` + `KEEP_LAST(10)` is advertised to a stock peer as
`KEEP_LAST (1)`, and a late joiner receives one sample, not ten. The grant is
truthful and the boot log says so once:

    qos: publisher '/qos_chatter' asked for TRANSIENT_LOCAL KEEP_LAST(10); this
    backend retains 1 sample and replays it on a late joiner's query. Granting 1
    and advertising it to the graph.

Measured 2026-10-06 on `native_rust_qos` / `native_c_qos_talker` against
`rmw_zenohd` (Humble) with `ros2 topic info -v` — issue 1687.

## Why it is filed

Issue 1687's four red cells were exactly this number: the demos declared
KEEP_LAST(10), the wire said 1. 1687 was fixed by making the demos declare the
depth this backend serves. The limitation itself is unchanged: an application
that needs the last N samples latched (a map, a parameter snapshot history)
cannot get it on zenoh, while `subscriber_ring_depth` is already DERIVED from
the largest declared subscription depth (phase-454 W6.a).

## Shape of a fix

`shim/publisher.rs::transient_local` names it as the extension point: make the
depth a knob, derived like the subscriber ring from the largest declared
transient-local publisher depth in the sizing descriptor, and keep a ring of
that many retained samples per `RetainSlot`. The pool is priced
(`MAX_TL_PUBLISHERS × (TL_RETAIN_BYTES + …)`), so the cost multiplies by the
depth and must be reported the way the other derived pools are. The query
callback then replies once per retained sample, oldest first.

## Acceptance

A transient-local publisher declaring KEEP_LAST(N) within the derived bound is
advertised as KEEP_LAST(N), and a late-joining stock subscriber receives N
samples.

## Resolution

Resolved 2026-10-07 on `fix/1709-tl-retain-depth-derived`, the shape this
issue proposed.

- `nros_sizing_descriptor::transient_local_retain_depth` — the deepest
  `KEEP_LAST` any transient-local publisher declares (an action server counts
  1 for `/status`); refuses on `keep_all`, a missing depth, or a refused
  transient-local count. One spelling, beside `max_subscription_depth`.
- `nros-rmw-zenoh/build.rs` — `ZPICO_TL_RETAIN_DEPTH`: a stated knob wins,
  the declaration supplies the default, the builtin stays 1; floored at 1 at
  this consumer. Priced in `scripts/pool-inventory-knobs.txt` as a factor of
  `TL_SLOTS`.
- `shim/publisher.rs` — each `RetainSlot` is a static ring of that depth (per
  publisher, no heap), replayed OLDEST FIRST; an oversize publish drops the
  whole history rather than leave a stale tail under a KEEP_LAST(N) promise.
- `zpico_query_reply_keep` (C, cbindgen header, Rust decl) — a reply that
  keeps the stored query open. A series ends with a plain `zpico_query_reply`,
  so the reply slot is freed exactly once; a failed reply of either kind
  releases it (issue 0902's leak).
- `shim/qos.rs` still grants `min(asked, TL_RETAIN_DEPTH)` and advertises it;
  the clamp message names the knob.
- `examples/workspaces/features`: `rust_qos.contract.yaml` declares
  `/qos_chatter`'s TRANSIENT_LOCAL KEEP_LAST(5), and the talker asks for 5
  again (issue 1687 had lowered it to the 1 the backend could serve).
- Proving it exposed issue [1740](1740-lifecycle-node-slot-uncounted.md): a
  contracted lifecycle image's node table was one slot short. Fixed here.

**Measured** (`native_rust_qos` vs a private `rmw_zenohd`, Humble; a late
`ros2 topic echo --qos-durability transient_local --qos-depth 5`, arrival
times relative to its first sample; `ros2 topic info -v` for the advertised
profile):

| build | advertised publisher depth | late joiner's first second |
| --- | --- | --- |
| origin/main (talker declares 1) | `KEEP_LAST (1)` | 1 sample (45), then 1 Hz |
| this fix (derived `TL_RETAIN_DEPTH = 5`) | `KEEP_LAST (5)` | 14, 15, 16, 17, 18 within 2 ms, then 1 Hz from 19 |
| this fix rebuilt with `ZPICO_TL_RETAIN_DEPTH=1` | `KEEP_LAST (1)` | 2 samples (1 retained + 1 live) |

Tests: `qos_override_e2e::a_late_joiner_receives_the_declared_history` and the
advertised-depth assertion pass on the derived build (4/4) and both FAIL on
the `ZPICO_TL_RETAIN_DEPTH=1` build ("got 2 sample(s) in its first second").
Unit: the descriptor rule (4), the ring's oldest-first arithmetic and
`retain` keeping at most the depth (run at depth 1 and at 4), the qos grant.

Sweep: `git grep -n "TL_RETAIN_DEPTH" -- packages`

**Not measured.** A nano-ros SUBSCRIBER as the late joiner (its history query
now gets several replies into its ring; only stock `rmw_zenoh_cpp` was run).
The Zephyr image `zephyr_rust_qos`, which reads the same launch file and so
now carries the contract: not built or run here beyond tier 2's build lane.
Zephyr/NuttX and the C/C++ roads derive the depth through the same descriptor
function, but none was run. The RAM cost on an embedded image (`MAX_TL_PUBLISHERS
× depth × (TL_RETAIN_BYTES + attachment)`) was not `mem-report`ed.
