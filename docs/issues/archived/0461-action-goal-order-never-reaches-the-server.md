---
id: 461
title: "An action server reads `1` for every goal's `order` — the request payload never reaches `ctx.message()` in `on_goal`"
status: resolved
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

## Root cause (2026-08-07) — TWO bugs, and the filed guess was wrong

The issue guessed "a constant `1` points at something structural, e.g. a CDR
encapsulation word". Half right, and the wrong half mattered: `1` is the GOAL
COUNTER. `goal_id_from_counter` writes `counter.to_le_bytes()` into the first
eight bytes of the uuid, so the first goal's uuid begins `01 00 00 00` and a
reader positioned on the uuid decodes `order = 1`. The second goal would have
read `2` — the "constant" was an artifact of only ever looking at goal one.

Dumping the wire settled it. The example client sent **28** bytes:

```
0..4    00 01 00 00   outer CDR header
4..20   01 00 …       goal_id uuid (counter)      <- Rust read order here  -> 1
20..24  00 01 00 00   a SECOND CDR header         <- C/C++ read order here -> 256
24..28  0a 00 00 00   order = 10
```

while the `action-client-multigoal` fixture sent **24** — `[header][uuid]
[fields]`, no inner encapsulation. Two clients, two framings, which is why the
concurrent-server fixture decoded `order=40` correctly and the example never
decoded anything correctly.

**Bug 1 — the client double-encapsulated.** `TickCtx::send_goal` serialized the
goal with `CdrWriter::new_with_header` and handed the result to
`send_goal_raw`, which frames `[header][uuid][<those bytes>]` itself. ROS 2's
`Fibonacci_SendGoal_Request` is ONE message with ONE encapsulation, so the
24-byte form is correct and the example was emitting something a real
`rcl_action` peer would misparse. Fixed by writing fields only
(`CdrWriter::new`) — the typed `ActionClient::send_goal` handle always did.

**Bug 2 — the Rust goal callback read the uuid.** `CallbackCtx::message()` does
`new_with_header` and deserializes, which for a goal payload lands on the
goal_id. Independent of bug 1: with a correct 24-byte request it still reads the
counter. Fixed by skipping the uuid when the context carries a goal decision —
the same skip the typed `try_accept_goal` path has always done, which is the
proof it is the right shape.

C and C++ needed no change: their trampolines strip the framing and prepend a
fresh header, which is correct once the client stops adding one.

## What I got wrong on the way

I first "fixed" this server-side, changing the arena dispatch, both raw
accessors and the typed path to a new `goal_offset`. All three languages then
read `order 10` — and `action_multigoal` went from 4 accepted goals to 0,
because the typed path serves clients using the OTHER framing. Chasing the
symptom at four seams produced a fix that looked complete and broke a passing
test; the wire dump is what turned it into two one-line changes at the two
places actually responsible. I also lost time to a stale C build behind a stale
CLI, which presented as "deserialize failed" — a fifth wrong lead.

## Verification

Same client against all three server languages:

```
C     order 10   result: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
CPP   order 10   result: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34]
RUST  order 10   result: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
```

(C++'s ten elements are its example's exclusive loop bound, not the wire.)

`actions` + `action_multigoal`: **5 passed**, including the multigoal table
test that the wrong fix broke.

## The guard

`goal_order_reaches_the_server` asserts the value ROUND-TRIPS, via
`output::ACTION_GOAL_ORDER` + `output::goal_order_in`. Every pre-existing action
test asserted delivery markers, which is why three languages could each decode a
different wrong number for months while the suite stayed green.
