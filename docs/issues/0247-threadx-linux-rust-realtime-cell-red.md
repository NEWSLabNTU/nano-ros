---
id: 247
title: "realtime_tiers_e2e threadx_linux_rust: /ctrl counter 0 on a fresh image (pre-existing; baseline-verified)"
status: open
type: bug
severity: medium
area: threadx
related: [issue-0246]
---

## Finding (2026-07-24, during the phase-296 W5.10 preempt-threshold work)

`realtime_tiers_e2e::case_15_threadx_linux_rust` FAILS (~8.7 s):
"high-tier /ctrl counter 0 is not ≥3× the low-tier /telem counter" — the
spawned `high` (ctrl, 10 ms) tier publishes NOTHING while the boot `low`
(telem) tier delivers.

**Baseline-verified pre-existing:** with the W5.10 changes stashed
(threshold declaration + board markers) and the fixture lane rebuilt from
clean tree, the cell fails identically. Not the preempt-threshold work.

Notes:
- The phase-297 W5 note says a boot-reprioritize fix landed for exactly
  this starvation shape (app@4 starving high@5) — either it regressed, is
  incomplete on a fresh rebuild, or the phase-297 agent's work is still in
  flight. Coordinate with phase-297 before debugging independently.
- The W5.10 marker e2e (`threadx_preempt_threshold_applied`) PASSES on the
  same image — bring-up + the `low` boot tier work; specifically the
  SPAWNED high tier's publish path is dead (#246/#245 family: check
  executor-storage sizing + the chain-spawn path before assuming a race).

## Debugging session 2 (2026-07-24) — substantially narrowed

Instrumented the ctrl component (temp `log::info` on tick) + manual boots
with router + sinks:

- **The ctrl timer FIRES at full rate** (~100 Hz, counter monotonic) on the
  spawned `high` tier, and `publish_to_topic` returns **Ok** every tick —
  yet the host sink on `/ctrl` receives ZERO. `/telem` (boot tier, same
  session) delivers at exactly its rate simultaneously.
- So the failure is WIRE-SIDE, silent: puts accepted, nothing leaves (or
  nothing routable leaves) for that publisher.
- `Z_FEATURE_MULTI_THREAD` is **1 and effective in the library** (the
  platform manifest `defines_kv` reaches the unified builder; `_z_mutex_*`
  symbols linked in the image) — the earlier "single-threaded zenoh raced
  by two tiers" theory is DEAD. (The generated header's `#ifndef` fallback
  0 is cosmetic; the -D wins.)
- Prime suspect: the **per-publisher interest-based write filter never
  opens** for publishers declared on the SPAWNED tier — zenoh-pico
  short-circuits puts (returns OK) when its filter says no matching
  subscriber. The boot tier's publisher (telem) opens fine; the spawned
  tier declares later and its filter state may never see the router's
  subscriber-interest reply (reply consumed/mis-associated when BOTH tier
  threads drive `zp_read` via the ThreadX select arm?). The #144 comment
  documents exactly this failure shape ("the losing publisher's write
  filter stays closed and every put is silently dropped").
- ZENOH_DEBUG=3 via the platform manifest produced no extra output —
  tracing needs a different hook (zenoh-pico log sink on threadx).

**Next:** trace `_z_write_filter` ctx state for the ctrl publisher
(gdb-from-start with a breakpoint on `_z_write_filter_callback` /
`_z_trigger_interest`, or a temp printf in filtering.c), and compare the
interest IDs in the router's debug log against the two publishers. If
confirmed, the fix likely belongs in the zpico threadx spin arm (interest
replies must be processed before the spawned tier's declare completes —
extend the #144 serialization to cover filter-open) or in zenoh-pico's
filter/interest association under concurrent readers.

## Repro

```
bash scripts/build/workspace-fixtures-build.sh threadx-linux rust
cargo nextest run -p nros-tests -E 'test(threadx_linux_rust)'
```
