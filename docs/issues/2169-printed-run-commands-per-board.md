---
id: 2169
title: "The run command `nros build` prints is pinned by shape, not run on every tier-1/2 board"
status: open
type: test-gap
severity: low
area: [cli, build, testing]
related: [phase-517]
found: 2026-10-31
---

**Home: [phase-517](../roadmap/archived/phase-517-user-flow-setup-build-flash.md)** — W6's done-when, "a command that, pasted, runs
the image", holds only for the shapes the tests pin.

## What

`nros build --board <b>` (`cmd::build_next`) prints the platform's next
command after a build: the QEMU line, the simulator binary, `west flash`,
and so on. The unit tests in `build_next.rs` pin each command's SHAPE per
board, and they share the QEMU arguments with the test harness through
`nros-qemu-ladder` (W14). The probe tracks boot the images their chapters
name, and the chapters' commands are written from the printed lines. Nothing
pastes the printed line ITSELF, on every tier-1/2 board in
`board-support.toml`, and checks that it starts the image.

A board whose printed line drifts from the line the chapter shows, such as
a flag added in one place only, would pass every gate.

## Fix direction

* For each tier-1/2 board with a probe track, have the track run the command
  `nros build` printed (captured from its output) instead of, or beside, the
  chapter's literal line, and assert the same expected output.
* A board with no track (FVP, SITL, flash-only hardware) states that, in the
  same table, rather than counting as covered.
