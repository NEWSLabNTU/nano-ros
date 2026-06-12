---
id: 43
title: C++ action server returns an empty result for a goal sent by the C action client
status: open
type: bug
area: c-api
related: [phase-239]
---

Cross-language action interop is **asymmetric**. Pairing the C++ callback action
client against the C action server works end-to-end (full Fibonacci result +
feedback — `test_action_callback_interop_cpp_client_c_server`). The reverse — the
**C** action client against the **C++** action server — does not: the C client
gets goal acceptance (`Goal accepted!`) but the final result comes back

```
Final result (status=SUCCEEDED): []
```

i.e. `status=SUCCEEDED` with an **empty** sequence. Same-language pairings are
fine (`C↔C` and `C++↔C++` both deliver the full sequence), so the callback
receive model itself is wire-compatible; the defect is specific to the **C++
action server** handling a **C-framed** goal / get-result exchange.

## Likely cause

The empty (length-0) result corresponds to the server computing the goal as
`order=0`. The C++ action server's inline Fibonacci callback computes the result
from `goal.order`; an empty result implies it parsed `order=0` from the C
client's goal request. The C and C++ clients both frame the goal via the shared
`ActionServerCore` (`send_goal_raw` → `[CDR_HDR][uuid(16)][order]`), so the
divergence is in how the C++ server's typed goal trampoline
(`goal_trampoline` → `GoalType::ffi_deserialize`) deserializes a C-client-framed
goal vs a C++-client-framed one — analogous to the now-fixed init/degraded-session
class of bug but on the goal-deserialize seam. The get-result reply path
(goal-id matching / result CDR) is the other suspect.

## Repro

Spawn `cpp_action_server` (zenoh) + `c_action_client` against the same locator;
the C client prints `Final result (status=SUCCEEDED): []`. Phase 239.15 left
this pairing untested (only the working cpp-client↔c-server direction is in
`native_api.rs`).

## Fix sketch

- Instrument the C++ server's `goal_trampoline` to log the deserialized
  `goal.order` for a C-client goal vs a C++-client goal; if it reads 0, the
  goal-deserialize offset/framing is the bug.
- Once fixed, add `test_action_callback_interop_c_client_cpp_server` (assert the
  C client's `Result (status=SUCCEEDED): [0, 1, 1, 2, …]`).
