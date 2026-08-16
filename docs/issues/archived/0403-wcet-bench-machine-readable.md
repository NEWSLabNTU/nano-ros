---
id: 403
title: The WCET bench emits prose nothing parses, and a QEMU run with a dead
  cycle counter reports zeros as if they were measurements
status: resolved
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

## The second half is DONE 2026-08-16 (phase-356 W2, first item)

The bench no longer manufactures data it did not measure.

Before: it detected the dead counter, printed a NOTE, measured anyway, printed a
full table of zeros, `[PASS]`, and exited SUCCESS. The warning and the data went
to the same stream and only the data was shaped like something a tool would
read — so the zeros are what survived into the log.

Now it refuses: no table, a diagnostic that states WHY zero is dangerous rather
than merely that the counter is dead, and `EXIT_FAILURE`.

```
FAIL: the DWT cycle counter is not counting.

      Every measurement would be 0, which is indistinguishable
      from `this operation is free` — and 0 is the most optimistic
      WCET there is, so consuming it always errs toward
      `schedulable` (issue 0403 / issue 0259).

      QEMU does not implement DWT cycle counting. Run this on
      real hardware; there is nothing to measure here.
[FAIL]
```

Verified: `just qemu test-wcet` exits 1 on QEMU, and the run emits **zero** rows
matching `min=0 max=0 avg=0` (was one per benchmark, 13 of them).

### `just qemu test-wcet` now FAILS on QEMU, and that is the point

QEMU does not implement DWT cycle counting, so this bench cannot measure there
at all. The recipe failing is that fact, reported. It is `[group("debug")]` and
no CI lane runs it, so nothing that gates a change goes red; `just qemu
test-all` aggregates it and will now report the failure, which is correct — it
was previously reporting a pass for a run that measured nothing.

## The first half is DONE 2026-08-16 — the machine-readable artifact

### Why it is split across a binary and a script

`wcet-cycles-qemu` is `no_std` on Cortex-M and its only output channel is
semihosting stdout: it cannot open a file, so it cannot write an artifact
itself. What it can do is print each number twice — once as prose for a human,
once as an `NROS_WCET_V1`-marked TSV record for a tool. A host-side script turns
the marked records into JSON.

That split is also what makes the work testable. The producer needs hardware,
and there is none here; the parser needs only text.

* `packages/testing/nros-bench/wcet-cycles-qemu/src/main.rs` — emits
  `NROS_WCET_V1` records: one `measurement` per benchmark (name, min, max, mean,
  iterations) and the conditions `counter_valid` / `cpu` / `profile` / `commit`.
* `.../build.rs` — bakes `NROS_WCET_PROFILE` and `NROS_WCET_COMMIT`, each
  falling back to `"unknown"` rather than failing a build outside a checkout.
* `scripts/bench/wcet-log-to-json.py` — parses those out of the log into a
  `nros.wcet.measurements/1` artifact.
* `just qemu wcet-artifact [log]` — runs it against `test-logs/latest/`.

Direction items 1 and 3 are covered: per-measurement min/max/mean/iterations and
identity, plus the conditions. `counter_valid` is carried into the artifact, so
"a stale file cannot be re-read as good" holds on the consumer side too.

### The two absences it refuses to paper over

**A refused run gets no artifact.** A log with no measurements is not an
artifact with zero measurements — that is this issue's own second half, one
layer further out. The script exits non-zero and writes nothing.

**No clock rate means not convertible.** The bench cannot read the part's real
clock, so it emits none; `clock_hz` stays `null` and the artifact says
`convertible_to_time: false` beside it. A consumer needing `ms` must refuse such
a file rather than pick a plausible rate. Inventing one is precisely the
manufactured-WCET failure 0404 exists to prevent.

### Verification, and the part that is NOT verified

Tested: `--self-test` covers 8 cases — a well-formed log, the absence of
`clock_hz` suppressing the convertibility claim, a refused run yielding no
measurements, prose containing the text `min=0 max=0 avg=0` NOT being parsed as
data, a malformed record raising rather than being skipped, and an unknown
record kind raising. The self-test runs on every conversion, not only when asked
for. Run against the real refused QEMU log, the script exits 1 and writes no
file.

**The emitter has never been observed emitting.** On QEMU the bench refuses
before it reaches any marker line — the observed run produced **0** of them —
and this host has no hardware lane, so no run anywhere in this tree can
currently produce an artifact. The producer is therefore compile-checked and
format-checked, not executed.

Format-checked means: `producer_format_drift()` reads the bench's actual source
and asserts the marker and the measurement record's field count still match what
the parser expects. Without that, both sides could agree with a hand-written
fixture forever while drifting from each other — the mirror-drift class this
tree has been bitten by repeatedly (the sizes-header mirror 0088→0268, the FFI
struct mirrors 0160). It does not substitute for a real run; it removes the one
failure mode a real run would otherwise be the only way to catch.

That guard was proven by sabotage rather than assumed to work — a guard nobody
has seen fire is a comment. Dropping one field from the bench's measurement
record: `the measurement line emits 4 values, but this parser expects 5`.
Renaming the marker: `no longer emits NROS_WCET_V1 — the producer and this
parser have diverged, so every future log will parse as empty`. Both exit 2.
The self-test was also re-run AFTER `just format`, because a formatter that
rewraps the producer's line is exactly what would silently unhook a check that
reads source text.

**What a hardware owner should confirm:** that a run on a part with a live DWT
produces marker lines, that `just qemu wcet-artifact` converts them, and that
`clock_hz` gets a real value from somewhere — the bench cannot read it, so
convertibility to `ms` remains unproven end-to-end.

### What this does NOT do

It does not populate 0259's mapper. The granularity caveat above stands: this
bench measures primitives, the mapper wants a whole callback's `exec_ms`, and
primitives do not sum to a callback without a model of one. 0404 now has a
producer to design a schema against, which is what it was waiting for — not a
finished input.
