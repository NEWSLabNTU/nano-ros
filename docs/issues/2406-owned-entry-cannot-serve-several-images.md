---
id: 2406
title: "A hand-written `main` built for several images has no workspace shape: an owned entry is one image's program"
status: open
type: limitation
severity: medium
area: [cli, build, testing]
related: [phase-531, rfc-0121, rfc-0122, rfc-0071, rfc-0065, 1556]
found: 2026-11-10
---

**Home: [phase-531](../roadmap/phase-531-workspace-first-project-layout.md) W6.**
It blocks the W6 sites whose program keeps its own `main` and is built more than
once. A decision is needed before they move.

## What

W6 migrates a standalone package into a node package plus a bringup. A `main`
that only calls `nros::main!` is deleted, because the generated entry writes
that call. Any other `main` is moved and reported (RFC-0121 D8, RFC-0122 D10).
There are two ways to keep such a `main`, and neither covers a program built for
more than one image:

- **An owned entry.** It is ONE image's program, by rule:
  `leaf_system::claiming_image` refuses an entry claimed by two images ("an
  entry is ONE image's program"), and its adapter
  (`generated/nros-derived/<entry>/`, RFC-0071 D13) is written per entry, from
  that one image. On the cargo road the entry is also found only at
  `src/<image>_entry` (`cmd/build.rs`, the `hand_written` check);
  `[image.<id>] entry` is read only by the west road
  (`west_application_dir`).
- **A component.** The generated entry boots it, so the `main` is gone. That
  changes what the program is, and for several sites the `main` is the subject.

The sites, measured 2026-11-10:

| Site | Images or variants | Why the `main` stays |
| --- | --- | --- |
| `packages/testing/nros-tests/bins/action-server-concurrent` | `linux` (zenoh, `zenoh-reply-slots`) and `linux_xrce` | opens its own executor and drains `get_result` every spin; the zenoh build reads `nros_rmw_zenoh::reply_slot_refusals_total` |
| `packages/testing/nros-tests/bins/message-info-observer` | plain and `unstable-zenoh-api`, both zenoh | uses the executor's `.message_info()` builder, which `CallbackCtx` does not expose |
| `examples/linux/c/talker` | zenoh, xrce and cyclonedds | `an_rclc_c_application_writes_a_census_without_a_router` (issue 1556) tests an rclc application that owns its `main` and spin loop, and this is the binary it runs |
| `examples/linux/cpp/talker` | zenoh, xrce and cyclonedds | `a_cpp_application_writes_a_census_at_its_first_blocking_call` (issue 1556) runs its `rclcpp::init_in` `main`, whose first blocking call is `rclcpp::spin_once` |

The two bins also carry the configured rows RFC-0121 D9 was meant to move
(`zenoh-reply-slots`, `unstable-zenoh-api`). D9 turns on a NODE package's
features on the generated entry's dependency line, so it does not reach an
owned entry's own `[features]` either.

## Options

1. **Let one owned entry serve several images.** `[image.<id>] entry` names it
   on every road, the claim rule allows several images of one bringup, the
   adapter is written per image (for example under the image's `build/<coord>/`
   and reached through the per-image settings file), and `node_features` on such
   an image turns on the entry's own features. This is a change to RFC-0071 D13
   and to the claim rule.
2. **One owned entry per image.** `src/linux_entry`, `src/linux_xrce_entry`,
   and so on, each a copy of the `main`. It works today and duplicates code.
3. **Make them components, and move the subject elsewhere.** The census test
   would need another rclc application with its own `main`, and the bins would
   be rewritten against the component API, if it can express them.

Option 1 is the only one that keeps both the `main` and a single copy.

## Decision (maintainer ruling, 2026-11-11): option 1, a multi-image entry

One owned entry may serve several images. RFC-0071 D13 and the
`claiming_image` rule are amended to allow it, and the cargo road locates an
owned entry by its declared name (`[image.<id>] entry`), not only at
`src/<image>_entry`. How an image turns on the owned entry's own `[features]`
is decided in the RFC amendment. The two bins and the linux C and C++ talkers
then migrate, and issue 1556's census tests keep running the same `main()`s.
