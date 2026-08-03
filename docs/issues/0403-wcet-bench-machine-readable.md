---
id: 403
title: The WCET bench emits prose nothing parses, and a QEMU run with a dead
  cycle counter reports zeros as if they were measurements
status: open
type: enhancement
area: testing
related: [0259, 0404, rfc-0047]
---

## Problem

`packages/testing/nros-bench/wcet-cycles-qemu` is the only thing in the tree
that measures execution cost, and nothing can consume what it produces.

It prints to semihosting stdout:

```
  cdr_serialize_int32: min=0 max=0 avg=0 cycles
```

via `print_result()` (`src/main.rs:379`), into
`logs/latest/qemu-wcet-bench.log`. There is no parser, no schema, and no
consumer. `just qemu test-wcet` sits in the `debug` group, so no CI lane runs
it and no artifact outlives the log.

That was tolerable while the numbers were informational. Issue 0259 makes them
load-bearing: `ros-launch-manifest`'s chain-aware mapper sums a per-boundary
`exec_ms` to decide whether a chain is schedulable, and as of rlm v0.1.4 it
warns whenever it had to count a boundary as zero. Filling those in requires a
producer, and this bench is it.

## The second half: a zero is not a measurement

On QEMU the DWT cycle counter does not increment, so every number the bench
prints is `0`. The bench detects this and says so:

```rust
if !dwt_active {
    hprintln!("NOTE: DWT cycle counter reads as 0 (QEMU limitation).");
    hprintln!("      Cycle counts will be 0. Validate on real hardware.");
}
```

and then measures anyway, prints a full table of zeros, and exits 0. The
warning and the data go to the same stream, and only one of them is shaped
like something a tool would read.

This is exactly the failure 0259 is about, one layer earlier. There, an absent
WCET entered the arithmetic as zero and a chain looked maximally feasible.
Here, a run with no working counter produces zeros that are indistinguishable
from "this operation is free" — and zero is the most optimistic value a WCET
can take, so the mistake always errs toward declaring things schedulable.

## Direction

1. **Emit a structured artifact** — JSON or TOML beside the log, carrying per
   measurement `min` / `max` / `mean`, `iterations`, and the identity of what
   was measured. Prose stays for humans; the artifact is what any future
   declaration (0404) is generated from.
2. **Make a dead counter a hard failure.** If `dwt_active` is false, exit
   non-zero and emit no measurements at all — not zeros, not a note. A run
   that cannot measure has produced no evidence, and the artifact must be
   incapable of expressing "measured zero" when it means "did not measure".
   Record the counter's validity in the artifact too, so a stale file cannot
   be re-read as good.
3. **Record the conditions.** A cycle count means nothing without the CPU, the
   clock rate, the build profile, and the commit. Cycles convert to the `ms`
   the mapper wants only through a clock rate, so an artifact without one is
   not convertible.

## Granularity caveat

The bench measures library primitives — CDR serialize/deserialize, `crc32`,
publish, `SafetyValidator::validate`. The mapper needs `exec_ms` for a timer
boundary, i.e. a whole callback. Primitives do not sum to a callback without a
model of the callback, so this bench alone cannot populate the mapper even
once it emits parseable output. Whether the declaration is measured per
callback on hardware, or composed from primitives, is the schema question and
belongs to 0404 — noted here so the producer is not mistaken for a complete
answer.
