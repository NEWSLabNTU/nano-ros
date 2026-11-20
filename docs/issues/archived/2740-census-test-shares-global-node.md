---
id: 2740
title: "A census test reads the process-global \"current node\" while sibling tests in the same binary open nodes, so under load a parameter lands on another test's node"
status: resolved
type: flaky-test
severity: low
area: [testing, census, metadata]
related: [issue-2221]
found: 2026-11-19
resolved: 2026-11-19
resolved_in: "90191a6869 (#4394)"
---

## What happened

On 2026-11-19 at 13:11Z, a full `just ci gate` of an unrelated branch went
red at step 8 (`check::build`). The only failing gate was
`build::check::census-hooks-complete`, which runs `nros-rmw-metadata`'s
`component_registration` tests under `cargo test`:

    a_launch_parameter_read_before_create_node_lands_on_that_node --- FAILED
    panicked at packages/rmw/metadata/tests/component_registration.rs:1242:9:
    `late_2589` is on the reader's node: {"node":"talker", ... "name":"late_2589", ...}

`late_2589` was filed under node `talker` instead of `reader_node_2589`. Run
alone, the test passed 3 of 3, both on that branch and on main.

## Why

The census state the test reads, the node a parameter read is filed under
(`nros::census_hooks`), is process-global. `cargo test` runs a test
binary's tests as threads of one process. At least six sibling tests in the
same file open a node named `talker` (lines 105, 427, 453, 498, 621, 1147).
One of them opening its node between the test's `create_node("reader_node_2589")`
and its `param("late_2589")` moves the "current node", and the read is
attributed there. More load makes that window more likely.

This is a reading of the code, not a captured interleaving, but it is the
only producer of `talker` near the failing assertion.

## Fix direction

No retries (CLAUDE.md): make the tests that read or write the census's
global state hold one lock for their whole body, or reset and own that state
per test. Then the test cannot observe another test's node.

## Fix

90191a6869 (#4394) fixed it in the test binary, not the product. The census
recorder stays process-global by design (one program per process, whose
sched tasks may create nodes on several threads). Every recording test in
`component_registration.rs` takes a `Census` guard first and holds it to its
end. The guard is one process-wide mutex that tolerates poisoning.
`metadata_executor` and `record` take the guard by reference, so no test can
open a recording executor without holding it. The type system is the sweep:
all 24 tests in the binary are guarded.

Measured with 16 test threads per run: 3 of 300 runs failed before the fix,
0 of 300 and 0 of 2000 after.
