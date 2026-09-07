---
id: 1036
title: "Arena exhaustion was half silent, and on the island it was wholly
  unreachable -- the two diagnostics written to explain it go to a sink that
  does not exist"
status: open
type: bug
area: core, boards, testing
severity: high
found: 2026-09-04
related: [issue-0900, issue-0589, phase-412, phase-403, phase-409]
---

## What happens

Two separate defects that compose into one blind spot, both found while
building phase-412's boot self-report.

**Half of arena exhaustion reported nothing.** `Executor::arena_alloc` has
called `report_arena_exhausted` since issue 0900 -- it names the knob and the
shortfall, because `NodeError::BufferTooSmall` is what a dozen other paths
return and a bare return code cannot distinguish them. Its sibling
`arena_alloc_with_trailing` returned the same error and said nothing:

```rust
let new_used = trailing_offset + trailing_bytes;
if new_used > self.arena.len() {
    return Err(NodeError::BufferTooSmall);   // <- and that was all of it
}
```

The split is not random. `arena_alloc_with_trailing` is the path for the
BUFFERED subscription entries and the action entries -- `spin.rs:3949`,
`spin.rs:4124`, `spin.rs:4348`, `action.rs:1454`. Those are what an island
image actually allocates, so the silent half is the half that matters on the
board that hit it.

**And on that board, the loud half is silent too.** Both diagnostics go through
`nros_log`. On the MR-CANHUBK344 the console is on `lpuart0`, which is not
wired; `lpuart2` carries the zenoh serial transport and cannot take a second
protocol. So the two messages written specifically to explain an arena failure
reach nothing, on the one board where the arena was being derived.

## Why it was not caught

Every hosted test has a sink, so the message is asserted and passes
(`executor_arena_advisory.rs` does exactly that). The gap is only visible on a
target with no sink, and there is no cell for that -- the assertion is "the
advisory reaches a sink", which is untestable where the answer is "there is no
sink".

The `_with_trailing` half is worse: no test asserts anything about it, because
it produced no output to assert on. It is the shape issue 0196 names -- a
diagnostic nobody can observe is indistinguishable from one that was never
written.

## Measured

Reading `git grep arena_alloc` across `packages/core/nros-node/src`: 20 call
sites, of which 8 go through `arena_alloc_with_trailing`. Those 8 covered every
buffered subscription and every action entry.

On the island, the practical consequence is recorded in phase-412: the derived
configuration produced a degraded ROS graph, the node count gave 4, 0, 0, 4, 4
across five runs of one unchanged config, and no channel could say why. RTT was
tried and could not discriminate -- a working image and a derived image both
printed only the Zephyr banner, and a deliberate positive control (`MAX_CBS=1`)
produced nothing at all.

## Fixed here, one half of it

`arena_alloc_with_trailing` now calls `report_arena_exhausted`, so both halves
name the knob. That closes the asymmetry but not the reachability: on a board
with no sink the message still goes nowhere.

For the reachability half, phase-412 landed `boot_report` -- a fixed 60-byte
RAM record read back with a debugger rather than a log stream, because the
failure halts before any stream could carry it. `note_alloc_failed` records the
first failing allocation and its shortfall, so a dump names the number to add.

## Not fixed, and this issue stays open for it

**No cell exercises the target-side path.** The record is unit-tested on the
host (`boot_report::tests`), and `check-boot-report-layout` keeps the decoder
in step with the struct, but nothing yet builds an image with
`CONFIG_NROS_BOOT_REPORT=y`, exhausts its arena on purpose, and asserts the
dump names the shortfall. Until that exists, the instrument is verified in
every part except the one that runs on silicon -- which is the same shape as
the defect it was built to find.

**The sibling question is unswept.** `report_arena_exhausted` and
`report_arena_headroom` are two of an unknown number of diagnostics that assume
a sink. Nothing has enumerated which other `nros_log` call sites are reachable
only on a target that cannot carry one.

## 2026-09-05 — the "unit-tested on the host" claim was not true either

This issue's resolution rests on the record being verified on the host, with the
silicon path left open. Measured today: **no lane sets `NROS_BOOT_REPORT`.**
`git grep` over `just/`, `justfile` and `.github/` finds it only in
`config-knob-census.py` and in `read-boot-report.py`'s own usage text. The cfg
is set from an env var, so unlike a cargo feature it cannot even arrive by
`--workspace` unification — every test in `boot_report::tests` had run zero
times in CI.

Same defect class as `sim-time` (phase-425) and the `env` tests (issue 0687),
and `check node-std-tests` exists precisely for it. Both now run there.

### And the gap that mattered was one layer further in

Those unit tests exercise the RECORD — layout, magic, monotonic stage. Nothing
exercised the LINK: that the allocator, on failing, actually writes the number
an operator would dump. That link is the whole instrument. On the board this was
built for the console UART is not wired, so the record is the only channel, and
a record nobody writes to is indistinguishable from the silence it replaced.

`executor::tests::arena_exhaustion_reaches_the_boot_record` closes it, through
`arena_alloc_with_trailing` specifically — the half that was silent, and the
half carrying every buffered subscription and every action entry. It asserts
four things: the failure is recorded at all, the SHORTFALL is non-zero (that is
the actionable number, not a flag), the shortfall is a plausible difference
rather than an arbitrary value, and a SECOND failure does not overwrite the
first — the allocation that explains the boot must survive later incidental
ones.

Mutation-checked: deleting the `note_alloc_failed` call from the `_with_trailing`
path fails the test.

### One thing the writing of it found

The test cannot share a process with `boot_report::tests`. The record is a
process-global static keeping only the first failure, and that module has a
`note_alloc_failed(100, 8)` case — with both in one binary the link test read
`(100, 8)` and its "nothing recorded before me" precondition fired. It is two
cargo invocations for that reason, stated in the recipe.

### STILL open, and unchanged

No cell exercises the TARGET-side path: an image with
`CONFIG_NROS_BOOT_REPORT=y`, an arena exhausted on purpose, a dump read back
with the decoder. That needs a Zephyr SDK, and the host this was written on has
none — the same wall issue 1075 hit today. What has moved is that the host side
is now genuinely verified rather than nominally: the record is written by the
real allocator on the real failure path, and a lane runs it.

## 2026-09-07 -- the record named the number and could not name the place

Both halves were re-measured against the tree rather than against this file,
and the "half silent" half was still half silent, one layer in from where the
first fix landed.

### Stage 4 was declared, documented, decoded, and emitted by nothing

`Stage::RegisteringEntities = 4` is documented as "the interval where an
under-sized arena halts", and `read-boot-report.py` shipped it as
`RegisteringEntities (NOT YET WIRED -- no call site emits this)`. Measured:
`git grep` finds four `checkpoint` call sites in the tree -- `ReportReady`,
`BootConfigResolved`, `ExecutorReady`, `FirstSpin`. Stages 4 and 5 had no
producer.

So an image that ran out of arena wrote a record whose `stage` read
`ExecutorReady` -- the same value an image reports when it opens an executor
and dies before registering anything at all. The record named the ALLOCATION
and could not say WHERE, which is exactly half of a diagnostic on the one board
where it is the only channel. The module's own claim, "the stage that was NOT
reached names the phase to look at", was false for the single phase this
instrument was built to observe.

Fixed: `boot_report::note_alloc` and `note_alloc_failed` stamp
`RegisteringEntities`. The stamp is on the RECORD's writers rather than on the
two `arena_alloc*` sites, because the arena is claimed by entity registration
and by nothing else -- so every present and future call site is covered by
construction and there is no second place to remember. On the failure path it
is stamped unconditionally, outside the first-writer branch: a second failure
adds nothing to the numbers but is still evidence that registration was in
flight, and a stage that depended on winning a race is the sort of number this
record must never print.

Stage 5 (`EntitiesReady`) has no truthful producer in the core and is now
documented as RESERVED rather than left reading as an oversight. "Every entity
the image declares was registered" has no observable moment here: an
application may register lazily, which is the same reason issue 0900's headroom
advisory fires at the first spin instead of at an end of registration that does
not exist. Removing it would renumber `FirstSpin` and bump `VERSION` to retire
a value no image can produce; the next reader would then file the renumbering
as the bug. An entry shape that DOES know when its register pass ended -- a
generated component `setup` callback returning OK -- can stamp it later without
moving anything.

### The log channel had never been asserted for the `_with_trailing` half

This file says "every hosted test has a sink, so the message is asserted and
passes (`executor_arena_advisory.rs` does exactly that)". That is not what that
file asserts. It asserts `report_arena_headroom` -- the OVER-PROVISION advisory,
a different function on a different latch. Nothing anywhere asserted
`report_arena_exhausted` firing at all, from either half. Measured by mutation:
deleting the `report_arena_exhausted` call from `arena_alloc_with_trailing`
passed every test in the tree.

### The dump-and-decode path had no test either

`check-boot-report-layout` compares two SOURCE files. `boot_report::tests` reads
the record through `Snapshot`, in Rust. Neither can see a dump, so the tool an
operator actually runs had never been run against a record produced by a real
failure -- in any lane, on any host.

`executor::tests::an_exhausted_arena_decodes_to_the_knob_an_operator_must_set`
closes that, as far as a host can take it. One real arena exhaustion through
`arena_alloc_with_trailing`, then:

* the log line reached a sink, names `NROS_EXECUTOR_ARENA_SIZE`, and was not
  truncated by `nros_log`'s 256-byte format budget;
* `read-boot-report.py --addr-only` resolves the record by SYMBOL out of the
  test binary's ELF, and the size the symbol table reports equals
  `BootReport::struct_size()` -- those are the two numbers a `savemem` line is
  built from, and a short dump is the one failure this record cannot report
  about itself;
* the record's own BYTES (not a re-serialised `Snapshot`, so the compiler's
  layout rather than the test's idea of it) decode under the script;
* the verdict exits non-zero and names ARENA EXHAUSTED, the stage
  `RegisteringEntities`, and `set NROS_EXECUTOR_ARENA_SIZE >= <capacity +
  shortfall>` -- the value, not the symptom.

Mutation-checked both ways: removing the `report_arena_exhausted` call fails it
on the log channel, removing the `RegisteringEntities` stamp fails it on the
stage.

### Gate and lane

* `check-boot-report-layout.py` now also compares the `Stage` ladder -- every
  variant's NAME and NUMBER -- against the decoder's `STAGES` table, with the
  same three negative controls the field-order check has. A renumbered stage is
  the drift that DECODES: the script prints a plausible phase name for a number
  the image never meant. The gate's reach was narrower than the rule it
  enforced, which is the issue-0196 shape.
* `just check node-std-tests` runs the new cell as a third `NROS_BOOT_REPORT=1`
  process, and every filtered invocation in that recipe now goes through one
  shared `ran_tests` guard. A cargo filter that matches nothing runs zero tests
  and exits 0; the backing-latch check had a hand-written guard for that and the
  two boot-report invocations had none.

### STILL open, and this is the whole of what is left

**The on-silicon run.** No Zephyr image has been built with
`CONFIG_NROS_BOOT_REPORT=y`, had its arena exhausted on purpose, halted, and
been dumped with `pyocd commander savemem`. Nothing in this repo can do it:
there is no hardware-in-the-loop lane (`git grep` over `just/`, `.github/` and
`scripts/` finds no probe harness), and the board and the shared sizes-probe
directory were in use by another task while this landed, so no board build was
attempted -- deliberately, rather than for want of a toolchain.

What has moved is that the step the probe performs is now the ONLY untested one.
Symbol resolution, dump length, positional decode, the operator's verdict and
both diagnostic channels are exercised by a lane on every run. The board run is
one command against an image, and the expected output is written above.

**The sibling sweep, unchanged.** Nothing has yet enumerated which other
`nros_log` call sites are reachable only on a target that cannot carry a sink.
