---
id: 246
title: "realtime_tiers_e2e nuttx_arm_rust cell times out on a fresh image (pre-existing; baseline-verified); riscv trio precondition-skips"
status: open
type: bug
severity: medium
area: nuttx
related: [issue-0245]
---

## Finding (2026-07-24, during the phase-296 W5.9 sporadic-server work)

`realtime_tiers_e2e::case_10_nuttx_arm_rust` **times out (60 s)** — solo and
in-sweep — on a freshly built `ws-realtime-rust` NuttX arm image, while the
sibling `case_08_nuttx_arm_cpp` and `case_09_nuttx_arm_c` cells PASS (~13 s)
on equally fresh images.

**Baseline-verified pre-existing:** with the W5.9 changes stashed and the
rust lane rebuilt from clean tree, the cell times out identically. Not the
sporadic-server work (`apply_tier_sporadic` is also a no-op for this fixture
— its nuttx tier declares no budget/period).

#245's lesson applies: a timeout that looks like a hang may be a crash or a
config-sized-storage bug — the Rust nuttx arm shares the executor arena
sizing story with its board glue; start there and with a manual QEMU boot
(`--seed`ed, with a router + observers, per the archived-0245 debugging
notes) before assuming a delivery race.

**Also:** the `nuttx_riscv` trio currently precondition-skips —
`workspace-fixtures-build.sh nuttx-riscv rust` reports "No workspace
fixtures matched platform=nuttx-riscv rust" while the test expects
`ws-realtime-rust/target-fixtures/nuttx-riscv/.../riscv_nuttx_entry`; the
riscv rust fixture is built by a different lane name (find + document it, or
fix the row's platform key). The riscv cpp/c rows built fine with the
`nuttx-riscv` arg but their cells still skipped-red in the same sweep —
re-verify after the rust lane question is settled.

## Note (2026-07-24b)

Likely the SAME family as #247 (both are the RUST multi-tier arm over the
zpico shim; the cpp/c siblings ride nros-cpp's zenoh and pass). #247's
debugging established: timer+publish healthy, wire-silent, MULTI_THREAD=1
effective — per-publisher write-filter suspect. Triage #247 first; re-test
this cell after any filter/interest fix.

## Repro

```
bash scripts/build/workspace-fixtures-build.sh nuttx rust
cargo nextest run -p nros-tests -E 'test(case_10_nuttx_arm_rust)'
# TIMEOUT 60 s solo; cpp/c siblings pass in ~13 s
```
