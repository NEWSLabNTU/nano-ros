---
id: 2043
title: "An ament-road port declares no entities, so every pool sits at its embedded default and start-up dies one pool at a time"
status: open
type: bug
area: [cmake, cpp, sizing]
severity: high
found: 2026-10-25
related: [1556, 1407, phase-463, phase-488, phase-518, 2070]
---

## What

A stock ROS 2 C++ package built through the ament verbs (`add_subdirectory(<nano-ros>)`
plus `NanoRosAmentSurface.cmake`) has no `system.toml`, no SystemModel, and no
`nros sync`. None of the three sizing-descriptor producers (RFC-0100 D4)
reaches it. So every pool keeps its embedded default, and a real node outgrows
them in turn. Autoware's `autoware_external_cmd_selector` @0.45.0 (12
subscriptions, 7 publishers, 1 service, 2 timers, plus `diagnostic_updater`)
failed at start-up in this order, one rebuild apart:

1. `ExecutorFull` at the fourth subscription (`NROS_EXECUTOR_MAX_CBS` defaults to 4);
2. `liveliness: declare failed (Full)` (`NROS_ZENOH_MAX_LIVELINESS`);
3. the publisher, subscriber and queryable tables (`NROS_ZENOH_MAX_{PUBLISHERS,SUBSCRIBERS,QUERYABLES}`).

It ran, and a stock Humble `rmw_zenoh_cpp` peer saw all 19 topics, the service
and the parameters, only after five env knobs were raised by hand at build time:
`MAX_CBS=40 LIVELINESS=128 PUBLISHERS=SUBSCRIBERS=QUERYABLES=32`. A porting
user cannot know that list in advance. Phase-488 W3 made the executor's
message name `NROS_EXECUTOR_MAX_CBS`. The zenoh messages name retired
spellings (issue 2042).

## Measured on the RTOS roads (2026-10-26, phase-518 W4)

The same missing declaration reaches a fact that is not a pool. On Zephyr,
`CONFIG_NROS_RMW_LOCAL_QUERYABLE` (zenoh-pico's same-session query path) is
DERIVED from the entity inventory: on only when the image declares both a
service client and a service server. A ported ros2/demos `composition` on
`mps2_an385` declares neither, so it derived OFF. The ROS 2 peer's
`ros2 service call /add_two_ints` was answered (`sum=5`), but the in-image
client's requests never reached the in-image server: 19 timer ticks, 0
`Got result`. With `[image.zephyr.knobs.rmw] local_queryable = 1` stated by
hand, the client got 17 results in the same run. FreeRTOS was not affected,
because its CMake road defaults the feature on.

Node 1 (`autoware_external_cmd_selector`: 12 subscriptions, 8 publishers, one
service, two scalar parameters) on mps2-an385 FreeRTOS needed four facts stated
by hand before it ran. The census already knows each one:

- `NROS_RMW_SUBSCRIBER_SLOTS=16`. The default of 8 refused the ninth
  subscription, and the node aborted in its constructor.
- `NROS_MAX_STRING_VALUE_LEN=64 NROS_MAX_ARRAY_LEN=4 NROS_MAX_BYTE_ARRAY_LEN=64`.
  At the defaults, declaring one `double` cost ~70 KiB of stack, and the
  image's 64 KiB app stack overflowed into the kernel's globals (issue 2070).
  The node declares no array parameter. `nros-params/build.rs` already reads a
  per-capacity NEED from the descriptor, so a census-fed descriptor would
  answer these.
- The heap (`NROS_FREERTOS_HEAP_KB=2048`). The 3 MiB default plus the C++
  executor storage did not fit in 4 MiB of SRAM.
- The executor arena. The boot report said `11112/723968 bytes claimed at first
  spin` against `NROS_EXECUTOR_MAX_CBS=40`. That is 98 % of 724 KiB unused, and
  it is the storage that pushed the heap out.

So the census answer must also feed the derived SWITCHES the inventory decides,
not only the pool sizes. On an RTOS it must come from somewhere other than the
image, which cannot be run on the host. A host build of the same packages is
the obvious source.

## Decided (2026-10-24)

The maintainer ruled that the sizes are derived from the program, as issue
1556's census does for a standalone C/C++ leaf. They are not raised to a
generous hosted default, and the user does not state them.

## Design sketch

- **The census already runs on this road's binary.** A hosted C++ image is
  built with `metadata-mode` (`NanoRosFeatureSet.cmake`: cpp, posix, not
  cross). `NROS_CENSUS_OUT` selects the recording backend, and the census run
  opens its executor at `CENSUS_SIZING` on the heap (issue 1419). So the
  undersized first build can be censused: no router, and no rebuild at census
  sizing.
- **Take it after the link.** A `POST_BUILD` step on each ament executable
  (`ament_auto_add_executable`, the `rclcpp_components_register_node`
  EXECUTABLE) runs it once with `NROS_CENSUS_OUT=<build>/nros/census/<exe>.census.json`,
  using the same `run_census` machinery as `nros ws entity-census take --leaf`.
- **Size from it on the next configure.** The census file is a
  `CMAKE_CONFIGURE_DEPENDS` input. Its counts become the sizing descriptor the
  runtime crates read (`NROS_SIZING_DESCRIPTOR` on the Corrosion command),
  through a `--from-census <file>` producer beside `--from-leaf`. A stale or
  incomplete census is refused per field, never guessed.
- **The open question is the first build.** It cannot be right-sized before
  it exists. Either the build reruns itself once (the census changed, so the
  runtime is rebuilt, which costs one runtime build on first configure), or
  the first image boots and fails with a message naming the census and
  "build again". The first is what "derive from the program" promises a user
  who types `colcon build` once.

## Done when

Porting a stock node that needs more than every embedded default
(`autoware_external_cmd_selector` is the measured case) takes no sizing knob:
`colcon build` (or `cmake --build`) produces an image that starts, and its pools
match its census. A fixture or `just check` lane measures this on an in-tree
ament package that creates more entities than the defaults, and fails if a
pool is left at its default.
