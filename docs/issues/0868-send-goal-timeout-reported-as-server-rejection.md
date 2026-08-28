---
id: 868
title: "A `send_goal` TIMEOUT prints as `Goal was rejected by server`, so an
  intermittent XRCE action failure reads as a deterministic server decision"
status: open
type: bug
area: examples, testing
---

## The diagnostic defect (the part worth fixing)

`examples/native/cpp/action-client/src/main.cpp:81` reports every non-OK
`send_goal` as a server rejection:

```cpp
ret = client.send_goal(goal, goal_id);
if (!ret.ok()) {
    fprintf(stderr, "Goal was rejected by server (order=%d, ret=%d)\n", order, ret.raw());
```

`ErrorCode::Timeout` is `-2` (`packages/api/nros-cpp/include/nros/result.hpp:36`),
so a goal whose response never arrived prints:

```
Goal was rejected by server (order=10, ret=-2)
```

The server rejected nothing. It may never have received the goal at all. The
message names a specific server decision — and `on_goal` does have a reject path
(`order out of range`) — so it sends a reader to the wrong file. It cost exactly
that here: the observed failure was in the XRCE service/action inbox path that
had just been rewritten, and the message pointed at `on_goal` rather than at a
timeout.

Five copies carry the line, and they are a portability group, so the fix moves
all five together:

```
examples/{native,qemu-arm-freertos,qemu-arm-nuttx,qemu-riscv64-threadx,threadx-linux}/cpp/action-client/src/main.cpp
```

A rejection and a timeout should read differently — the rejection branch is
`ret.raw()` matching the reject code, everything else is "no response".

## The flake underneath it

`nros-tests::native_example_reqresp_e2e case_18_cpp_xrce_action` fails
intermittently in the FULL sweep only.

**Measured** (this host, 2026-08-28, on the tree at `d74cc3de0`):

| run | result |
| --- | --- |
| full `just ci` sweep (1541 tests) | FAIL — `ret=-2` |
| full `test-all` sweep (1573 tests) | PASS |
| solo, `-E 'test(case_18)'` | PASS x4 |
| whole `native_example_reqresp_e2e` suite, solo | PASS |

So: 1 failure in 2 full sweeps, 6 passes outside one. The other four XRCE cells
(08, 09, 16, 17) passed in every run including the failing sweep, which is what
argues against a payload or serialization defect — those share the same inbox
path.

**Reasoned, NOT measured:** that this is contention rather than a code defect.
The evidence is consistent with it (load-dependent, non-deterministic, siblings
green) but nothing here identifies *what* is contended. A goal response that
misses its window under parallel load could be the agent, the port, or the
client's own wait budget, and this issue does not distinguish them.

Not established either: whether it predates the phase-395 pull. It was not
observed in the sweeps run earlier the same day, but those were on a different
tree and the sample is far too small to date it.

## Direction

1. Fix the message first — it is cheap, it is a five-copy class, and until it is
   fixed every future occurrence of this flake will be mis-triaged the same way.
2. Then re-measure the flake with a message that distinguishes the two, and a
   sweep repeated enough times to give the rate meaning. One failure in two runs
   is a report, not a rate.
