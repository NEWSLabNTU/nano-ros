---
id: 246
title: "realtime_tiers_e2e nuttx_arm_rust cell times out on a fresh image (pre-existing; baseline-verified); riscv trio precondition-skips"
status: resolved
type: bug
severity: medium
area: nuttx
related: [issue-0245, issue-0247]
---

## Resolution (2026-07-24 — phase-296 follow-up)

Fixed in `nros-board-nuttx` (`src/lib.rs`, the Rust `run_tiers`). TWO distinct
defects on the Rust nuttx multi-tier arm, both surfacing as the same timeout
(the low tier never spawns / never delivers → the `/telem` anchor never reaches
5 → 60 s timeout):

1. **Spawned tier used the std default 2 MiB stack.** `std::thread::Builder`
   with no `.stack_size()` requests the libstd default (2 MiB); NuttX
   `pthread_create` cannot satisfy that (deterministic ENOMEM → "failed to spawn
   tier `low`"). The C/C++ sibling glue always passes an explicit
   `pthread_attr_setstacksize` (16 KiB / `stack_bytes`) because the executor
   arena lives on the heap (`nros_platform_alloc`) — the tier stack only carries
   call frames. Fix: the Rust spawn now sets `.stack_size(stack_bytes)` (honours
   `TierSpec::stack_bytes`, else 64 KiB = `CONFIG_PTHREAD_STACK_DEFAULT`). A
   residual TRANSIENT spawn failure under host/QEMU load (an `io::Error` with no
   OS errno) is absorbed by a bounded 5-attempt retry with a yield.

2. **The session-owning boot tier was budget-capped.** `resolve_tiers` sorts
   descending by raw priority, so on nuttx the boot tier is `high` (prio 110) —
   which the model declares `real_time` with a `budget_us`/`period_us`. The Rust
   `apply_tier_sched_policy` installs the lowered SchedContext as the executor
   **default**, gating EVERY dispatch on that executor — including the spin loop
   that flushes the ONE shared zenoh-pico session for all tiers. A sporadic
   budget there stalled the flush after a single sample (measured `ctrl=1`).
   The kernel `SCHED_SPORADIC` self-apply compounded it (drops the owner to
   `sched_ss_low_priority` on exhaustion). The C/C++ path never hit this because
   it binds the lowered context per HANDLE, leaving the boot flush Fifo. Fix:
   the session-owning boot tier keeps the default Fifo SchedContext and does NOT
   self-apply kernel sporadic (loud note when it declares a budget); the budget
   dim's kernel + cooperative realization applies to NON-owner (spawned) tiers,
   unchanged, in `nuttx_run_one_tier`.

`case_10_nuttx_arm_rust` now PASSES solo (6/6). Under a pathological
`yes`-hammered core the cell can flake on the 3× ratio (documented QEMU-icount
under-load jitter — retest solo), not on spawn. The `nuttx_riscv` trio question
below is untouched (still a fixture-lane-name mismatch — see below).

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

## Update (2026-07-24c) — NOT the same fix as #247

#247 (threadx_linux_rust) is RESOLVED, but its fix does NOT carry to this
cell — retested after #247 landed: still TIMEOUT. The #247 root cause was a
guest-side read drop on the ThreadX **select-driven spin-read** arm
(two tier threads sharing one `zp_read` on the TCP stream); the fix
serializes that spin read. NuttX uses a DIFFERENT zpico arm — background
read + lease TASKS (`Z_FEATURE_MULTI_THREAD=1`, `zp_start_read_task`), NOT
the select-spin path — so the ThreadX serialization is irrelevant here. This
cell needs its own investigation of the background-read-task + multi-tier
declare interaction (likely the same *class* — a spawned-tier publisher's
write-filter interest reply lost — but a different code path). W5.9b's
nuttx sporadic budget is NOT the cause: this cell timed out at filing,
before W5.9b, and the arm cpp/c cells (which also gained the budget) pass.

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
