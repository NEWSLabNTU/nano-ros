---
id: 462
title: "`workspace_features` rust/logging: the node's log lines carry no `[INFO]` level tag — 0 of an expected ≥3"
status: resolved
type: bug
area: runtime
related: [issue-0422, issue-0309, phase-264, phase-338]
resolved_in: phase-338
---

## Resolution

The defect was real and is fixed; what this issue captured was a **stale
fixture**, so the cell had already been green in source when it was filed.

The assertion names two candidate causes — the record bypassed the facade, or
it went through and lost its metadata. It was the second: every hosted `log`
bridge printed `record.args()` bare, with no level tag. Fixed in
`f0fa793f4` (nros-board-linux, the one this cell exercises) and in
`6863de1cc` for the other four sinks (nuttx, mps2, freertos, threadx), which
carried the identical defect.

## Measurement

Re-run on freshly rebuilt native fixtures (CLI + launch-resolver rebuilt first,
then `just build-test-fixtures lane=native`, so nothing in the chain was
museum state):

```
$ cargo nextest run -p nros-tests --test workspace_features_e2e
    PASS [90.062s] workspace_features        # all 17 cells
```

And the cell reproduced by hand, with `spawn_spinning`'s exact environment
(`RUST_LOG=info`, `NROS_SESSION_MODE=client`, `NROS_ENTRY_SPIN_MS=8000`)
against a router on the cell's own port 17881:

```
[INFO] nros: session open
[INFO] talker publishing chatter seq=0
[INFO] talker publishing chatter seq=1
...
marker lines: 8    of which tagged [INFO]: 8
```

8 of 8, against the 0 of ≥3 filed.

## Why it was filed after the fix had landed

`f0fa793f4` is an ancestor of the filing commit `8f3e11489`, so the tree that
filed this already contained the fix. The captured output settles which half
was stale — look at the FIRST line:

| | filed | after rebuild |
| --- | --- | --- |
| board-emitted | `nros: session open` | `[INFO] nros: session open` |
| node-emitted | `talker publishing chatter seq=0` | `[INFO] talker publishing chatter seq=0` |

`nros: session open` comes from the board bridge, not the node — it is tagged
or untagged purely as a function of which bridge is compiled in. Its being
untagged in the filed output means the **binary** predated `f0fa793f4`, i.e.
the `native_entry` fixture had not been rebuilt since. The node lines are
downstream of the same sink and untagged for the same reason.

This is the fixture mtime treadmill (CLAUDE.md): a pull or rebase refreshes
source mtimes, and a family that is not rebuilt afterwards keeps answering from
a museum binary. It is worth noting the failure did NOT present as `STALE` —
the staleness probe passed the fixture through and the assertion then failed on
its merits, which reads as a live runtime defect. That is the shape issue 0445
describes from the other direction, and the reason the cheapest first move on
any assertion failure is to confirm the binary postdates the fix you expect it
to contain.
