---
id: 2567
title: "Rust codegen puts `.msg` constants in a private module, so a node cannot name them"
status: resolved
type: bug
area: [codegen]
severity: low
found: 2026-11-14
resolved: 2026-11-15
resolved_in: "the change that resolved it (the packs and their tests, one change)"
related: [phase-478, rfc-0023]
---

GitHub: NEWSLabNTU/nano-ros#3859

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)**.
Seen on nano-ros 0.6.0-nros8, porting the island's message packages.

## What happens

Constants declared in a `.msg` file (for example `uint8 MODE_NOMINAL=0`) are
generated into a private Rust module and are not re-exported. A Rust node
cannot name them and has to repeat the literal.

## Direction

Make the constants reachable from the message type, e.g.
`IslandMode::MODE_NOMINAL`, as rclrs/rosidl do.

## Resolution

The `nros` Rust pack now emits a type's constants as associated consts of that
type: `IslandMode::MODE_NOMINAL`, as rclrs spells them. This covers a
message's `.msg` constants, a service's request and response constants, and
an action's goal, result and feedback constants (`ReachGoal::MAX_TARGET`).
The old module-level copies were inside the type's private module, so no user
could name them. They are removed. The action ones had been renamed
`<ACTION>_GOAL_<NAME>`, which is no spelling rclrs uses.

That prefix was the last caller of the packs' `snake_case` filter, so the
neutral filter set is retired. File naming still uses `utils::to_snake_case`.

Tests in `rosidl-codegen/tests/nros_test.rs`:
- `msg_constants_are_associated_consts_of_the_type`
- `srv_constants_belong_to_their_half`
- `action_constants_belong_to_their_part`

No committed generated code carries constants, so nothing in the tree needed
regenerating.
