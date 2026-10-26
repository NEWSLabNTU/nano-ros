---
id: 2043
title: "An ament-road port declares no entities, so every pool sits at its embedded default and start-up dies one pool at a time"
status: resolved
type: bug
area: [cmake, cpp, sizing]
severity: high
found: 2026-10-25
related: [1556, 1407, phase-463, phase-488, phase-518, 2070, 2073, 2086]
resolved: 2026-10-26
resolved_in: "PR #2712 (2026-10-27)"
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

Node 1 on Zephyr (`mps2_an385`) needed the same four, plus one more the
FreeRTOS road had not shown. Its nros heap (`CONFIG_NROS_ZEPHYR_HEAP_SIZE`)
defaults to 196 608 only when the image declares a parameter capability, and a
ported image declares none, so it got 65 536. The same node peaked at 179 976
bytes of heap on FreeRTOS. The image aborted at its fourth subscription with
`zpico Generic -> ConnectionFailed`, an out-of-memory reported as a transport
error. `[image.zephyr.knobs.zephyr] heap_size = 262144` fixed it.

Node 2 (Nav2 `nav2_velocity_smoother`) needed its pools stated on both boards
(executor 16 callbacks, zenoh tables 16, liveliness 128, the parameter
capacities), its heap (2048 KiB on FreeRTOS, 256 KiB on Zephyr) and a
192 KiB app stack on FreeRTOS (issue 2070). Its executor arena was 290 816
bytes on FreeRTOS and 1 157 120 on Zephyr, and the boot report said none of it
was claimed at first spin.

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

## Fix (2026-10-26, phase-518 W3)

A ported workspace states that its entities are what its program creates:
`[[component]] entities = "census"` in its `system.toml`, the opt-in issue
1556 gave a standalone leaf. Four pieces make that work for an ament
workspace on both cmake roads:

- **The configure takes the census.** `nros_record_leaf_entity_facts`
  (NanoRosLeafEntityFacts.cmake) runs `nros ws entity-census take --leaf`
  when the census is missing or stale: a host build of the same sources into
  `<leaf>/build/census-host` (`-DNROS_LEAF_BOARD=linux`), run once. A current
  census returns at once. `NROS_LEAF_CENSUS_TAKE=OFF` leaves it to the caller.
  The add_subdirectory road reads the leaf's descriptor too (`CMakeLists.txt`,
  beside its leaf read); before, only the find_package road did.
- **The census build names its program.** An ament workspace adds its
  executables in packages outside its root, so `take --leaf` could not find
  `nano_ros_add_executable` in the root `CMakeLists.txt`. The host build now
  writes `<build>/nros/census-program.txt`, which lists every executable and
  every project source directory. nano-ros's own directories are left out,
  but the leaf's own tree never is. The verb runs the one executable, or the
  one the `[[component]] name` picks. The census is stale against the listed
  package directories.
- **The census is a complete observation, so the descriptor states every
  count.** The standalone-leaf road refused `callback_slots`,
  `publisher_count`, `entity_liveliness_tokens` and the rest, because a
  hand-written declaration was checked only for its queryables (issue 1378's
  rule, 1655). A census-backed leaf now uses its own horizon
  (`ModelHorizon::for_leaf_census`), which states them.
- **The census run gets the node's parameters.** Node 1 throws on an unset
  `initial_selector_mode`, so its census run aborted. Issue 2073's
  `params_files` / `ros_args` bake the launch parameters into the image, and
  the host census build inherits them. A board leaf's network files are
  skipped when `nros ws leaf-system --board` answers for the host
  (`leaf-system.rs`); the host refused them.

### Measured

- **Node 1, Linux, add_subdirectory road, no sizing knob.** The descriptor
  read `subscriber_count = 12`, `callback_slots = 21`, `publisher_count = 9`,
  `service_server_queryables = 7` and `entity_liveliness_tokens = 28`, and
  nros-node built `MAX_CBS = 21`. Against a stock Humble `rmw_zenoh_cpp`
  peer, all 19 topics, the parameter services and the select service were
  visible. The service returned `success=True`, `update_rate` read 10.0 (from
  the baked param file), and a remote gear command was forwarded.
- **The ament-workspace-port template, FreeRTOS (mps2-an385), find_package
  road.** The census was taken on the host (`1 pub / 1 timer`) and the cross
  image built `MAX_CBS = 1` from it. `cpp_port_ament_workspace_publishes_on_freertos`
  passed.
- **The gate.** `just check ament-census-sizing` builds
  `packages/testing/nros-tests/bins/ament-census-port`: six subscriptions,
  six publishers, two timers, and no size stated anywhere. It fails if the
  descriptor does not state the counts, or if nros-node's `MAX_CBS` is not
  the descriptor's value or is its default. Green: 14 callback slots against
  a default of 4. Without `entities = "census"` it fails. It runs in the
  merge group beside `template-copy-out`.

### Still open

- **Zephyr**: the module does not record the leaf's entities, so a ported
  Zephyr image is still sized by hand
  ([issue 2086](../2086-zephyr-ported-image-not-census-sized.md)).
- **The heap and parameter capacities** are not census answers. The census
  records parameters, but `[params]` stays `store = "none"` on this road, and
  the heap and app stack are a board's (issue 2070).
- **One census per image.** A workspace that builds several executables
  into one configure is censused for the one its `[[component]] name` picks.
