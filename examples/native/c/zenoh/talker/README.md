# nano-ros C Talker (POSIX + Zenoh)

Minimal C example: publishes `std_msgs/Int32` on `/chatter` once per
second over zenoh-pico against a local `zenohd` router. Used as the
canonical proof-of-concept for the Phase 137 source-distribution
consumption path.

## Build

Two consumption paths are supported. The first is canonical going
forward; the second is the legacy `install-local` flow being
retired in Phase 140.

### Canonical — `add_subdirectory(nano-ros)` (Phase 137)

The example's `CMakeLists.txt` pulls the nano-ros source tree
directly with `add_subdirectory`. No install step. Build with:

```bash
cd examples/native/c/zenoh/talker
cmake -B build -S .
cmake --build build
./build/c_talker
```

`CMakeLists.txt` is 20 lines. Platform + RMW selection happen at
configure time via two cache vars (`NANO_ROS_PLATFORM=posix`,
`NANO_ROS_RMW=zenoh`); both are set inline in this example because
they're the only valid combo for native zenoh-pico-via-pthreads.

Under the hood the `add_subdirectory(<repo-root>)` brings in:
- `NanoRos::NanoRos` — INTERFACE target wiring the RMW staticlib,
  the platform shim, and `nros_platform_link_app(<exe>)`.
- `nros_generate_interfaces(<pkg>)` — codegen function for ROS 2
  `.msg` / `.srv` / `.action` bindings; sourced from
  `<repo-root>/cmake/NanoRosGenerateInterfaces.cmake`.

The canonical path works with **no prior `just install-local`** —
the source tree IS the install layout. See
`book/src/getting-started/build-as-subdirectory.md` for the full
recipe and Phase 137's roadmap doc for the design notes.

### Legacy — `find_package(NanoRos CONFIG)` (pre-137, scheduled for removal in Phase 140)

The pre-137 flow built the nano-ros library set up-front into a
prefix (`build/install/`) and consumed it via CMake's
`find_package` mechanism. The flow still works today but is
slated for removal once every internal test and example switches
to the `add_subdirectory` path:

```bash
# 1. One-time: build the install tree (~30 archives across all
#    supported platform / RMW combinations).
just install-local

# 2. Build the example against the install tree.
cd examples/native/c/zenoh/talker
cmake -B build-legacy -S . \
    -DCMAKE_PREFIX_PATH="$PWD/../../../../../build/install"
cmake --build build-legacy
./build-legacy/c_talker
```

(The `find_package`-based `CMakeLists.txt` is not committed in
this example anymore. To exercise the legacy path, copy a
sibling example's pre-137 CMakeLists or read
`docs/roadmap/archived/phase-119-3-cmake-setup.md` for the older
recipe.)

The legacy path's drawbacks that motivated Phase 137:
- Up-front compile of every variant; users only need one.
- `find_package` against `/opt/nano-ros` doesn't fit `west` /
  `idf.py` / PlatformIO / NuttX-Kconfig / PX4 — the four
  workflows nano-ros actually targets.
- Two CMake APIs to maintain (`NanoRos::*` + `find_package`'s
  install-side machinery).

Phase 140 deletes `install-local` once every internal consumer
moves to `add_subdirectory`. Treat this section as a transitional
escape hatch, not a recommendation.

## Run

The talker expects a zenoh router (`zenohd`) on `tcp/127.0.0.1:7447`.
Start one in a sibling shell:

```bash
just zenohd
```

then run the talker:

```bash
./build/c_talker
```

Expected output:

```
[INFO] zpico_open: connecting to tcp/127.0.0.1:7447
[INFO] Publisher declared on /chatter (std_msgs/Int32)
[c_talker] Published: 0
[c_talker] Published: 1
[c_talker] Published: 2
...
```

Pair with the matching listener example at
`examples/native/c/zenoh/listener/` to verify end-to-end.

## Source layout

- `CMakeLists.txt` — 20 lines, `add_subdirectory(nano-ros)` consumer.
- `src/main.c` — publisher loop.
- `package.xml` — declares `std_msgs` dependency for `nros_generate_interfaces`.

## Related

- `book/src/getting-started/build-as-subdirectory.md` — the
  user-facing guide for the canonical path.
- `docs/roadmap/phase-137-source-distribution-entry-cmake.md` —
  design notes for the root entry CMake.
- `docs/roadmap/phase-140-install-local-rip-off.md` — the legacy
  path retirement plan.
- `examples/native/c/zenoh/listener/` — receiver-side counterpart.
