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
