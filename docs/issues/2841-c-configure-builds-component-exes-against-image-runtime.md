---
id: 2841
title: "A C image's configure also builds every typed C component's standalone executable against that image's runtime, so an image-only link requirement (the board clock hook) breaks unrelated executables"
status: open
type: bug
severity: low
area: [cmake, c, build, clock]
related: [phase-582, issue-2839]
found: 2026-11-22
---

## What happens

An image that changes the runtime it links gets its own configure. Stating
`[image.<id>.knobs.clock] source = "board"` gives the image a separate cmake
build dir (the C++ workspace's board image built under
`posix-zenoh-linux-cfg<hash>-pubsub_bringup`). In the C workspace, that
configure also builds each TYPED C component's own standalone executable
(`talker_pkg__talker`, `listener_pkg__listener`) against the image's
board-selected runtime. Those executables are not the image and do not link
the image's board package, so they fail with an undefined
`nros_board_wall_clock_ns`, referenced from
`nros_clock_board__set_by__knobs_clock_source.o`.

Measured 2026-11-22 while adding phase-582 W1d's stub-board images. The Rust
and C++ images build and pass `tests/linux_board_clock.rs`. C++ components
build no standalone executables, so the C++ configure is not affected. The C
image was dropped, and its absence is recorded in that test's doc comment.

## Why it matters beyond the clock

The board hook is the first runtime requirement that only the IMAGE
satisfies, and an image-specific configure treats every target in it as part
of the image. Any future image-only requirement (a board hook, a
per-image link input) will break the component executables the same way.

## Fix direction

One of:
- An image's configure builds only that image's targets (the entry and what
  it links), not every component's standalone executable. Those are built
  once, in the configure with the default runtime.
- Or each executable in that configure links the image's board package too.
  That treats a component executable as an image, which it is not.

The first matches what a configuration is (RFC-0066: a configuration is a
fixture axis, built where it applies). Acceptance: a C image with
`source = "board"` builds, and `tests/linux_board_clock.rs` gains its C case.
