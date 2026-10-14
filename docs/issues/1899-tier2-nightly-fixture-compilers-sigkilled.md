---
id: 1899
title: "Tier-2 nightly fixture build: compilers were SIGKILLed across five families at once, so crates fail with no diagnostic"
status: open
type: bug
area: [ci, build, testing]
severity: medium
found: 2026-10-14
related: [1353, 1365, 1898, 0115]
---

## Measured

Nightly run **38527151921** (schedule 07:15 UTC, main `756676e16`), job
**115730089599** `tier 2 nightly (pairwise cover)`, step
`just build tier2-nightly`, on `nano-ros-runner-newslab-118-server`. The job
was created at 07:15:29Z and waited 44 minutes for a runner, starting at 07:59:53Z (issue 1365).
Of the run's other jobs, 12 passed and 3 were skipped.

The fixture build ran with:

```
build-test-fixtures: budget=48, make-jobs=5, pool=4×12 + zephyr=48 (solo)
```

It failed in five families at once (nuttx, freertos, linux, qemu,
threadx_riscv64), and none of the failures carries a compiler diagnostic. The
log has no `error[E…]`. Instead:

```
sccache: Compiler killed by signal 9          (x3)
could not compile `core` (lib)
could not compile `serde_core` (lib)
could not compile `nros-platform-config` (lib)
could not compile `nros-node` (lib)           (x2)
could not compile `zerocopy` (lib)            (x2)
could not compile `ros-launch-manifest-model` (lib)  (x2)
```

Every `nros-c` consumer then panics in the size probe, at
`nros-build-helpers/src/shared.rs:61`:
`nros-c: size probe could not locate the `nros` rlib: … nested cargo build
failed … error: could not compile `<one of the above>``. On threadx_riscv64
the panic surfaces as `FAILED: [code=101] … _cargo-build_rv_virt_threadx_listener`.

Unrelated crates (`core`, `zerocopy`, `serde_core`) failing with no message,
beside three SIGKILLs reported by sccache, is the signature of the kernel
killing compilers under memory pressure. Disk is not the cause: the job
reported `923G free on the checkout's volume`.

## What this is NOT

- **Not 1353.** Disk was ample, and the shape is a killed process, not
  `No space left`.
- **Not 1898.** That issue is the 05:11 nightly's harness failing to *fetch* a
  git dependency. Here everything was fetched, and the compiles were killed.
- **Not a code defect in the crates named.** The same crates build in every
  merge-group gate, and the five families that failed have nothing in common
  but the machine and the moment.
- **Not proven to be the OOM killer.** The job log cannot show `dmesg`.
  Signal 9 from sccache plus diagnostic-free compile failures is strong
  circumstantial evidence, not a kernel record.

## What would close this

1. Confirm on `newslab-118-server`: the kernel log (`journalctl -k` /
   `dmesg`) between 07:59Z and 09:04Z on 2026-10-14 (the job's run window) should show `oom-kill` entries
   naming `rustc`/`cc1`. That needs a human on the host.
2. If confirmed, size the budget to memory, not to cores. Either derive
   `budget`/`pool` from `MemAvailable` (a rustc compile of `nros-node` costs
   GBs, and 48 concurrent ones may not fit), or cap the tier-2 nightly's pool
   on that runner. Measure peak RSS for one family before choosing a number.
3. Make the failure say so. A fixture build whose compile was SIGKILLed
   should print "killed (likely memory)" instead of letting a size-probe panic
   about a missing rlib be the first readable line.
