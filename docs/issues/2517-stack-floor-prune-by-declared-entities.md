---
id: 2517
title: "The stack walk counts entity kinds an image never declares, such as the 25 KB action-client registration in a talker"
status: open
type: tech-debt
severity: low
area: [memory, tooling]
related: [issue-2402, issue-2400, issue-2516, phase-557]
found: 2026-11-14
---

## What

A sound static walk counts every path that control flow can reach. In the
`nros` node runtime, `ExecutorSink::create_entity` matches on the entity's
kind at run time. So every kind's registration code is linked and reachable
from it, even in an image that declares only one publisher.

On `examples/mps2-an385-baremetal/rust/talker`, the walk's deepest path goes
through the action-client branch: an `out_of_line` instance whose frame is
**25,008 B**, then `register_action_client_raw_sized` (9,552 B). The talker
declares no action client, and its run (gdb watchpoint, issue 2400 W5b) takes
the publisher branch, whose `out_of_line` frame is 5,720 B.

## Two separate costs

1. **The walk is loose.** It counts branches the image never takes. If the
   walk knew the image's declared entity kinds, it could cut those branches.
2. **The frame is large.** A 25 KB frame for one registration is a real stack
   cost in every image that does register an action client. Shrinking it
   (move the buffers to the executor backing or the heap, as issue 2401 did
   for the session properties) helps those images, whatever the walk does.

## Fix

- **Pruning.** The walk takes the image's declared entity kinds as an input
  (the sizing descriptor's entity inventory, RFC-0100 D4), and treats the
  `create_entity` branches of undeclared kinds as unreachable.
  - Do this only when that input is COMPLETE. The descriptor states a field
    or refuses it, so a refused inventory means no pruning.
  - A branch is identified by its `out_of_line` closure, which needs a stable
    mapping from kind to closure.
- **Frame.** Measure the action-client registration's frame and move what
  dominates it off the stack.

## Acceptance

- The mps2 talker's walk no longer counts the action-client branch, and it
  still covers its run (the MPU guard bisect: overflow at 31 KiB, runs at
  32 KiB).
- An image whose inventory is refused is walked as today.
