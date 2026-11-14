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

## Parked (2026-11-15)

Measured before implementing, and parked: the gain is too small for the cost.

**Correction.** The 25,008 B `out_of_line` frame is the **Parameter** arm
of `create_entity`, not the action-client arm. Its callees are `nros_params`
drop glue and `Parameter` types. The action-client arm's frame is 904 B, and
its callee is `register_action_client_raw_sized`. The closures, in arm order,
are Publisher 5,720 B, Subscription 528, Timer 504, ServiceServer 664,
ServiceClient 368, ActionServer 1,296, ActionClient 904, Parameter 25,008.

**Method.** The walker ran on each image's real graph, with its real
arguments and the deepest-of-8 orders (issue 2550). Every `create_entity`
arm whose kind the image's sizing descriptor states as 0 was pruned:

| image | static now | static if pruned | floor saved |
| --- | ---: | ---: | ---: |
| mps2 bare-metal Rust talker | 95,420 | 89,548 (−6.2 %) | ~6.5 KB |
| nucleo Rust talker | 67,672 | 64,608 (−4.5 %) | ~3.4 KB |
| nucleo Rust action server | 68,608 | 68,608 | 0 |
| nucleo C talker | 99,140 | 99,140 | 0 |

- **The mps2 gain needs a parameter count the descriptor does not state.**
  All of it comes from pruning the Parameter arm. With that arm kept, the
  mps2 gain is 0. The mps2 talker's descriptor states only
  `[params] store = "none"`, with no `declared` count, while the nucleo
  talker's states `declared = 0`. Pruning the Parameter arm on mps2 would
  need that road to state the count first.
- **A C image cannot gain.** The C API does not go through the Rust
  `create_entity`.
- **Cost.** Kind tags on the eight arms, so the closures have stable names; a
  descriptor reader in the walker; a self-test; and possibly a new
  descriptor field.

The cycle in the bound graph (issue 2550, phase-557 W7f) inflates every image
and is the larger lever.

**Reopen when both hold:**

1. every road this would serve states the image's parameter count in its
   descriptor;
2. a re-measure shows a floor gain over 10 % on some image.

The frame half (shrink the Parameter registration's 25 KB) is independent
of pruning and stays open as written above.
