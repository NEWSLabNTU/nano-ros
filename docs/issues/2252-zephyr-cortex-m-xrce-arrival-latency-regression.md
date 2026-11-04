---
id: 2252
title: "Zephyr mps2/an385 XRCE arrival→dispatch exceeds its 5 ms bound (20.7 ms max, was 0.65 ms)"
status: open
type: bug
severity: medium
area: [zephyr, xrce, executor, testing]
related: [phase-466, 2251, 2221]
found: 2026-11-04
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-11-04: a deterministic red in the RFC-0113 arrival-latency cell; phase-521, which added the cell, is archived.

## What

`zephyr_cortex_m_qemu::zephyr_cortex_m_xrce_arrival_is_dispatched_within_the_port_bound`
fails deterministically:

```
arrival-latency: n=5 min_us=1296 mean_us=8039..9989 max_us=20747
```

The bound is `max_us <= 5_000`: one 1 ms tick plus the receive path, per
RFC-0113 D2.4 (`tests/zephyr_cortex_m_qemu.rs`, `run_arrival_latency_cell`).
When phase-521 W7 added the test (`6bfb96f700`), it measured XRCE at 323–647 us
over three serial runs, and zenoh at 435–877 us.

Five runs gave the same min and max. The run is under `-icount`, so this is
not load noise. The peer is discovered and all five samples are dispatched.
The Agent port (5784) and domain (218) are therefore correct, and the latency
is the only thing that moved.

## Window

- **PASS**, 2026-11-04 ~10:00: image built from `13358e690b` plus PR #3074's
  change (the XRCE domain 209→218).
- **FAIL**, 2026-11-04 21:37 onward: image built from `d5a0a0a65f` plus PR
  #3098's change.

#3098 moves no baked value: every emitted west record is byte-identical to
the bash formula's, signatures included. So the window is the 37 commits
`13358e690b..d5a0a0a65f`.

A path filter over the core, XRCE, Zephyr platform, mps2 board, cmake and
witness-bin trees names nothing Zephyr- or XRCE-specific. The one hit,
`bc5a0e947c`, touches FreeRTOS/ThreadX/Cyclone only, so the filter is too
narrow to point anywhere. Bisect, rather than guess.

The host peer differs too. The native Rust talker was rebuilt from the newer
tree for the failing runs. The image stamps receive-interrupt to dispatch
internally, so a peer change should not move it. That is untested: rebuild
only the image at `13358e690b` against the current talker before trusting it.

## Not checked

- The zenoh arrival cell. It failed in 0.1 s in the same session, most likely
  a fixture missing from a filtered build, so it says nothing either way.
  Re-run it on a full Cortex-M build.

## Next

1. Rebuild `build-cortex-m-c-idle-wake-xrce` at `13358e690b` and re-run, with
   the current host talker, to separate image from peer.
2. Bisect `13358e690b..d5a0a0a65f` on that one leaf. A single run is enough per
   step: the measurement is deterministic under icount.
3. Run the zenoh cell from a full Cortex-M fixture build.
