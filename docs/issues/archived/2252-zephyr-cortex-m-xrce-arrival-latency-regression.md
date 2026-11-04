---
id: 2252
title: "Zephyr mps2/an385 XRCE arrival→dispatch exceeds its 5 ms bound (20.7 ms max, was 0.65 ms)"
status: resolved
type: bug
severity: medium
area: [zephyr, xrce, executor, testing]
related: [phase-466, 2251, 2221, 2209]
found: 2026-11-04
resolved_in: "issue-2252 branch: measurement cells run a fixed -icount shift"
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-11-04: a deterministic red in the RFC-0113 arrival-latency cell; phase-521, which added the cell, is archived.

## Resolution (2026-11-05)

**Not an executor or XRCE regression. The cell measured QEMU's clock scaling, and
issue 2209's QEMU fix changed that scaling.** The receive path did the same work
throughout.

The window's only commit in the image's trees was `634bcc2b70`, issue 2209's
move to QEMU 11.0.0-nros8. In nros8 a Cortex-M `wfi` really halts. The witness
measures on the guest's own cycle counter, under `-icount shift=auto`. An idle
guest falls behind wall time, and QEMU's `icount_adjust` raises the shift to
its cap of 10, which charges 1024 ns per instruction. Under nros7, `wfi`
returned at once, the guest never idled, and the shift sat at 5.

The same image (`build-cortex-m-c-idle-wake-xrce`, built from `0573a1cf55`) was
run with the same host talker, changing only the QEMU binary and `-icount`:

| QEMU | `-icount` | arrival-latency (n=5) | cell |
| --- | --- | --- | --- |
| 11.0.0-nros7 | `shift=auto` | min=mean=max 648 µs | pass |
| 11.0.0-nros8 | `shift=auto` | min 1,296, mean 8,039, max 20,747 µs | fail (the issue's numbers, exactly) |
| 11.0.0-nros8 | `shift=4` | 324 µs | pass |
| 11.0.0-nros8 | `shift=5` | 648 µs | pass |
| 11.0.0-nros8 | `shift=10` | 20,747 µs | fail |

Each latency is about 20,260 instructions × 2^shift ns. The instruction count
does not move; only the price per instruction does. The idle-wake cell on the
same image read `max_late_us=4000` under nros8 `shift=auto` and `0` at
`shift=5`.

**Fix.** A cell that asserts a bound on a duration the guest measures now runs
with a fixed shift that models the board's core:
`nros_tests::qemu::MPS2_AN385_CORE_ICOUNT = "shift=5"` (32 ns, against 40 ns
for the 25 MHz Cortex-M3), via `QemuProcess::start_mps2_an385_networked_measured`.
All four witness cells in `zephyr_cortex_m_qemu.rs` use it: arrival and
idle-wake, over zenoh and over XRCE. `sleep=on` stays, so idle time runs at wall
speed and the session survives. Every other cell keeps `shift=auto`.
`docs/reference/qemu-icount.md` has a section on this.

Sweep: `grep -lE "QemuProcess|qemu::" packages/testing/nros-tests/tests/*.rs |
xargs grep -lE "max_late|max_us|latency|jitter|switch_ins"`. Only
`realtime_tiers_e2e.rs`, `sched_dims_applied_e2e.rs` and this file match. The
first two assert dispatch COUNTS and kernel throttling, not a duration on the
guest clock, so they are not this class.

The zenoh arrival cell, which "Not checked" below left open, had the same
defect. Its earlier 0.1 s failure was a stale native talker fixture. Once that
was rebuilt, the cell measured 1,754 / 8,699 / 16,741 µs (min / mean / max)
under nros8 `shift=auto` and failed. With the fixed shift it measured
876 / 878 / 881 µs and passed, consistent with the 435–877 µs phase-521 W7
recorded. The zenoh idle-wake cell read `max_late_us=2000` under `shift=auto`
and `0` with the fix.

Verified on 11.0.0-nros8, all four cells passing with the fix:

| cell | result |
| --- | --- |
| `zephyr_cortex_m_xrce_arrival_…` | max 648 µs |
| `zephyr_cortex_m_zenoh_arrival_…` | max 881 µs |
| `zephyr_cortex_m_idle_xrce_…` | `max_late_us=0` |
| `zephyr_cortex_m_idle_zenoh_…` | `max_late_us=0` |

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
