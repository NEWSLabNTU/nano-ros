---
id: 2841
title: "A C image's configure also builds every typed C component's standalone executable against that image's runtime, so an image-only link requirement (the board clock hook) breaks unrelated executables"
status: resolved
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

## Until it is fixed (manager ruling, 2026-11-22)

A component's own executable in a configure whose image names `board` is
REFUSED at configure (`_nros_carrier_board_clock_refusal`,
`cmake/NanoRosNodeRegister.cmake`), naming the knob, the executable and the
hook. Previously it failed later, linking a hook the user had not asked
about. Verdict rows `cmake_carrier_board_clock_refused` and its control
`cmake_carrier_board_clock_default_builds` (`tests/cmake_node_register_misuse.rs`).
The fix above removes the refusal.

## Resolved (2026-11-23)

The first fix direction: a workspace configure that exists for its images'
own configuration (its coordinate carries `-cfg<hash>`, i.e.
`image_config_digest` is `Some`) builds those images only.

- **The root says so.** The generated root of such a configure sets
  `NROS_CONFIGURE_BUILDS_IMAGES_ONLY ON` before `find_package(nano_ros)`
  (`CmakeRootSpec::images_only`, `builder/cmake_root.rs`).
- **The registration reads it.** The native typed-carrier branch of
  `nano_ros_node_register` (`cmake/NanoRosNodeRegister.cmake`) builds no
  carrier executable when it is set. The component libraries the generated
  entry composes are unchanged, and the default-runtime configure still
  builds the carriers.
- **The refusal stays for a leaf.** `_nros_carrier_board_clock_refusal` is
  still reached where the carrier IS the program (a standalone leaf with no
  generated root), so its verdict rows stand. The workspace road no longer
  reaches it.

The signal is the configuration digest, not `image_env`. `image_env` is also
true for an image whose only rows are the forced arch C flags (issue 2692),
which is not a configuration of its own.

Acceptance, measured on x86_64:
- The C workspace gained `clock_board_pkg` (the C twin of the C++ stub board)
  and `[image.linux_talker_clock_board]` with `[knobs.clock] source = "board"`.
  It builds (`build/posix-zenoh-linux-cfgf58aa09f66-pubsub_bringup-linux_talker_clock_board`):
  the image defines `nros_board_wall_clock_ns` and links
  `nros_clock_board_try__set_by__knobs_clock_source`, and the dir holds no
  `pkg/<pkg>/<exe>` executables.
- `tests/linux_board_clock.rs` gained its C case,
  `board_clock_reaches_the_wall_clock_and_the_source_stamp::case_2_c`: PASS.
  The C control `ordinary_talker_stamps_host_time_and_links_no_hook::case_2_c`
  PASSES against the default configure, which still builds
  `pkg/talker_pkg/talker` and `pkg/listener_pkg/listener`.
- Fixture row `workspace-c-native-talker-clock-board`, matrix cell
  `(Linux, C, Zenoh, Pubsub, Example) + [ClockBoard]`.
