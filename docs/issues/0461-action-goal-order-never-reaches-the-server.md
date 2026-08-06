---
id: 461
title: "An action server reads `1` for every goal's `order` — the request payload never reaches `ctx.message()` in `on_goal`"
status: open
type: bug
severity: high
area: rmw, actions
related: [issue-0450, issue-0445, issue-0047]
---

## Symptom

`examples/native/rust/action-client` sends `FibonacciGoal { order: 10 }`. The
server logs:

```
[INFO] Received goal request with order 1
```

Change the client to `order: 7`, rebuild, run again:

```
[INFO] Received goal request with order 1
```

The value is not merely wrong, it is **constant**. The server's
`ctx.message::<FibonacciGoal>()` inside the `on_goal` callback does not return
the client's goal payload.

Both runs are freshly built binaries against a live zenohd on an isolated port
(client mtime newer than its source; `strings` confirms the requested order is
compiled in).

## Why nobody noticed

The group-A action-server body read the order, logged it, and then **ignored
it** — it published a hardcoded `[0, 1, 1]` regardless (issue 0450). So the only
consumer of the mis-read value was a log line nothing asserted on. Every action
e2e passes today: they assert `Publish feedback` / `Goal succeeded` markers and
delivery, none of which depends on the order being right.

This surfaced only when 0450 made the server COMPUTE from the received order.
The result went from a plausible-looking fixed `[0, 1, 1]` to `[0, 1]` for a
requested order of 10 — the first output that depended on the value, and
therefore the first that could expose it.

Same shape as issue 0445: a value discarded quietly is a value whose wrongness
cannot be observed. The stub was not just under-demonstrating the example, it
was **masking a wire defect**.

## What is known

* The mis-read is on the SERVER side of the goal request. The client compiles
  the right value in.
* It reads a constant `1` for different requested orders, so this is not an
  off-by-N alignment slip that would track the input — it is reading a
  different field entirely. A `SendGoal` request is
  `{ goal_id: UUID (16 bytes), goal: { order: int32 } }`, so a reader positioned
  at the wrong offset would land inside the UUID (arbitrary per-goal, not
  constant) — a constant `1` points at something structural, e.g. a CDR
  encapsulation/length word or a status field, rather than at payload bytes.
* Not yet checked: whether the C and C++ action servers read the order
  correctly, and whether the same path is used by `on_accepted`. Those two
  answers bound the blast radius, and both are cheap.

## Why this is filed rather than fixed

It is a wire/deserialization defect in the action goal path, not an example
defect, and it wants an owner who can say which side of the
`ctx.message::<T>()` seam is wrong for action callbacks. Issue 0450 landed the
computation that makes it visible; leaving the demo publishing a fixed sequence
to hide it would be the wrong trade.

## Effect on the demo until this is fixed

`examples/*/rust/action-server` now streams the sequence for the order it
actually receives, which is `1` — so a client asking for 10 gets `[0, 1]`. That
is a faithful report of a broken input rather than a fixed output that looked
right. The e2e markers are unaffected.

## First checks

1. `ctx.message::<FibonacciGoal>()` in an `on_goal` callback — what buffer is it
   reading, and is it the SendGoal request or the wrapper?
2. Whether the C/C++ action servers show the same constant.
3. Whether `nros_tests`' action assertions can be extended to check the
   RESULT against the requested order, which is what would have caught this.
