---
id: 2405
title: "A hand-written C/C++ west application (`[image.<id>] entry = \"<pkg>\"`) gets no message-bound seed, so the promise check refuses every configure of it"
status: resolved
type: bug
severity: high
area: [cli, zephyr, cmake, build]
related: [phase-530, 2404, 1252, 2435, 2436]
found: 2026-11-10
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)**. W4 made the Zephyr module hook run `nros configure` and hold the configure to its seeds (the promise check, `cmake/NanoRosPromise.cmake`); this road was never seeded.

## What

A workspace bringup whose Zephyr image boots a hand-written C/C++ entry
package (`[image.zephyr] entry = "zephyr_entry"`, the entry's own
`CMakeLists.txt` calling `nano_ros_add_executable(... BRINGUP ...)`), built
with `west build ... -DNROS_IMAGE_SYSTEM_DIR=<bringup> -DNROS_IMAGE=zephyr`,
fails at the end of its first configure:

```
CMake Error at cmake/NanoRosPromise.cmake:399 (message):
  nros: this image's derived message-bound sizes was composed after this
  configure's readers ran, and no `nros configure` seeded them, so they used
  their defaults (Kconfig and crate values, which can be smaller than this
  image needs), while the late consumers of this configure read the composed
  answer (<build>/nros/message_bound_knobs.cmake).
```

Every configure of a clean build dir refuses the same way. Measured on the
simple-autoware-safety-island board image (MR-CANHUBK344, Zephyr 4.4) at
`5f6b2cd21a`.

## Why

`nros configure` seeds two answers a C/C++ west configure reads early: the
entity inventory (`resolved.cmake`, stage 3.5) and the message-bound sizes
(`resolved-message-bounds.cmake`, `message_bound_knobs::write_west_seed`).
The second is written in three places: `generate_entry` (a GENERATED C/C++
west application), `resolve_rust_west_image` (a Rust one) and the coordinate
union (the cmake roads). A west image with `entry` set never calls
`generate_entry` (`cmd/build.rs`: `if image.entry.is_some() ||
leaf_app.is_some()`), and the standalone leaf has its own seed
(`configure_zephyr_leaf`), so a hand-written workspace application is the one
west road with no bound seed. Its `nros_find_interfaces()` calls then compose
a derived answer the unseeded readers did not see, and the promise check
refuses it, as phase-530 W7 designed it to.

## Fix

Stage 4's west arm seeds a hand-written C/C++ application too
(`seed_hand_written_west_bounds`): the same `write_west_seed`, over the
image's launch closure (factored out of `resolve_rust_west_image` as
`image_launch_node_dirs`) plus the entry package itself, whose `package.xml`
names the interface packages its own `CMakeLists.txt` adds. The tables are
the metadata probe's; a table prices a type, so the probe's C++ tables serve
a C++ entry (`nano_ros_add_executable(... TYPED)`), with C as the fallback.
Never fatal, like the rest of stage 3.5.

Measured: the island board image configured and built in one pass after the
change (`message-bound sizes seeded from nros configure`), and its
RAM/ITCM/DTCM regions equal the pre-phase-530 image byte for byte.

With the seed in place the promise check does its job: the island's
`island_interfaces` package named `autoware_planning_msgs`, which no image
node uses, and the configure refused with "an interface package the host
configure finds that `package.xml` does not name" until the island dropped it.

Acceptance: a hand-written C/C++ west entry in a `src/` workspace configures
in one pass from a clean build dir.

## Resolution

Resolved 2026-11-11. The seed above landed in #3433. Its acceptance had no
in-tree road until this change, and building one found issue 2435, which hid
the seed again from any clean build dir.

Coverage: the `west_hand_written_c_entry` west fixture
(`packages/testing/nros-tests/fixtures/west_hand_written_c_entry`, a C
talker plus a hand-written `zephyr_entry`) is configured by raw `west build`
from a clean build dir, and `tests/west_hand_written_entry.rs` asserts that
the configure's `nros/message_bound_knobs.cmake` and
`nros/entity_inventory.cmake` are byte-equal to the seeds `nros configure`
wrote. The test asserts the files, not the configure's exit, because this
image derives no message sizes (std_msgs' closure is unbounded), so a missing
seed only warns here.

Two gaps on the same road are not fixed here; they are issue 2436.
