---
id: 2567
title: "Rust codegen puts `.msg` constants in a private module, so a node cannot name them"
status: open
type: bug
area: [codegen]
severity: low
found: 2026-11-14
related: [phase-478, rfc-0023]
---

GitHub: NEWSLabNTU/nano-ros#3859

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)**.
Seen on nano-ros 0.6.0-nros8, porting the island's message packages.

## What happens

Constants declared in a `.msg` file (for example `uint8 MODE_NOMINAL=0`) are
generated into a private Rust module and are not re-exported. A Rust node
cannot name them and has to repeat the literal.

## Direction

Make the constants reachable from the message type, e.g.
`IslandMode::MODE_NOMINAL`, as rclrs/rosidl do.
