---
id: 2843
title: "C++ `wait_for_service` inside a callback sleep-polls under the executor's lock, where the C API refuses"
status: resolved
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

## Resolution (2026-11-23)
Option 2, ruled by the project's manager session: the wait reads the flag once
through `CtxRef::refuse_in_dispatch`, takes the context for each poll and drops
it before the sleep. Inside a callback it now sleeps holding no lock; outside
one it spins under the lock as before. The call answers as it did.

Test: `tests_wait_lock::a_callback_waiting_for_a_service_leaves_the_executor_to_other_threads`
(nros-cpp lib, `rmw-cffi`) drives the C++ API over a stub backend with the
executor lock installed. A second thread must take the lock while a timer
callback's 600 ms wait runs. On the code before the fix it fails, "the second
thread got the executor lock 553 ms after asking, at 606 ms; the callback's
600 ms wait ends at 602 ms". With the fix it passes, 5 of 5.

`check-ffi-executor-derive` no longer lists this function: nothing reads
`in_dispatch` outside the refusals. Option 3, C parity, is
[issue 2848](../2848-c-wait-for-service-in-callback-parity.md), for the user.

