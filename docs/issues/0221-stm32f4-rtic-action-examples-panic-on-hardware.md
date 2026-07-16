---
id: 221
title: "stm32f4 rtic action-server/-client examples flash per their README but panic instantly — board init_hardware() is todo!()"
status: open
type: bug
severity: medium
area: examples
related: []
---

## Finding (deep audit 2026-07-17, J1)

`examples/stm32f4/rust/action-{server,client}-rtic/` READMEs walk the user
through build + flash with no warning, but the referenced board
`init_hardware()` path is `todo!()` — the image panics immediately on real
hardware. A copy-out example that compiles, flashes, and dies is worse than
an absent one.

## Fix sketch

Either implement the board bring-up for the rtic action pair (the pubsub
rtic examples have it), or gate the examples with a loud README warning +
`compile_error!` until the seam exists. Check whether the phase-289 RTIC
delivery work already provides the pieces.
