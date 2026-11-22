---
id: 2848
title: "Inside a callback, C refuses a blocking `wait_for_service` that C++ answers: should the two agree?"
status: open
type: limitation
severity: low
area: [c-api, cpp, executor]
related: [issue-2843, issue-2055, phase-573]
found: 2026-11-23
---
## What
Called from inside a callback, the same blocking wait answers differently in
the two front ends:

- C: `rclc_client_wait_for_service` refuses with `RCLC_RET_REENTRANT`
  (`refuse_in_dispatch`).
- C++: `nros_cpp_service_client_wait_for_service` sleep-polls the server's
  visibility until its deadline (issue 2055: ros2/demos' client calls it from
  a timer). Since issue 2843 it holds no executor lock while it sleeps.

A program ported between the two finds the call working in one and refused
in the other.

## Why it is a question, not a fix
Making C answer as C++ does changes what an rclc application sees: a call
that returns `RCLC_RET_REENTRANT` today would block for its timeout. That is
an rclc-facing API change, and RFC-0108 D9 names a blocking call inside a
callback as one that refuses. The user decides.

## Options
1. Leave them different, and document it in both API references.
2. C sleep-polls as C++ does, holding no lock across the sleep (the issue
   2843 shape), so a ported program behaves the same.
3. C++ refuses as C does, which undoes issue 2055's demo support.
