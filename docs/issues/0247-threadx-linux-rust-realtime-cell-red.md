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

## Repro

```
bash scripts/build/workspace-fixtures-build.sh threadx-linux rust
cargo nextest run -p nros-tests -E 'test(threadx_linux_rust)'
```
