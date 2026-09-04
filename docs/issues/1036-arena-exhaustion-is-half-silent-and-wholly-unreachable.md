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
