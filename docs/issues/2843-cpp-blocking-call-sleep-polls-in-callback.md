---
id: 2843
title: "C++ `wait_for_service` inside a callback sleep-polls under the executor's lock, where the C API refuses"
status: open
type: bug
severity: medium
area: [cpp, executor]
related: [issue-2055, issue-0290, issue-2675, phase-573]
found: 2026-11-23
---
## What
Called from inside a callback, the two front ends answer a blocking
service-availability wait differently:

- C: `rclc_client_wait_for_service` refuses with `RCLC_RET_REENTRANT`
  (through `refuse_in_dispatch` since phase-573 W3).
- C++: `nros_cpp_service_client_wait_for_service`
  (`packages/api/nros-cpp/src/service.rs`) reads `in_dispatch` and, inside a
  callback, sleep-polls `service_is_ready` until its deadline instead of
  spinning (issue 2055: ros2/demos' client calls it from its timer).

The C++ loop holds the context it took through `cpp_ctx_checked` for the
whole wait, so it sleeps holding the executor's lock. Inside a callback the
pass gave that lock back for the callback's window and the wait takes it again.
For the whole timeout no other thread can enter the executor. That includes a
sched task that shares it, and the owner of an owned pass the wait might be
waiting on.

## Why it is not fixed in phase-573 W3
RFC-0108 D9 says a blocking call refuses inside a callback. Following it here
breaks the ros2/demos pattern issue 2055 made work. Whether C++ refuses (and
the demo changes), the wait drops the lock between polls, or C learns the same
sleep-poll is a ruling, not a refactor. W3's sweep gate
(`check-ffi-executor-derive`) lists this function as the one reader of
`in_dispatch` outside the refusals, naming this issue, so the count cannot
grow while it is decided.

## Options
1. Refuse with `NROS_CPP_RET_REENTRANT`, as C does. The demo's timer call
   then fails and the demo must wait outside the callback.
2. Keep the sleep-poll, but take the context per poll and drop it across the
   sleep, so no lock is held while sleeping.
3. Give C the same sleep-poll, so the two agree, with option 2's lock rule.
