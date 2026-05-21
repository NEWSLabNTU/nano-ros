# nano-ros C Talker (POSIX + Zenoh)

Minimal C example: publishes `std_msgs/Int32` on `/chatter` once per
second over zenoh-pico against a local `zenohd` router. Canonical
proof-of-concept for the source-distribution consumption path
(Phase 137 / 140 / 144).

## Build

The example's `CMakeLists.txt` pulls the nano-ros source tree
directly via `add_subdirectory`. No install step, no install prefix.

```bash
cd examples/native/c/talker
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

See `book/src/getting-started/build-as-subdirectory.md` for the full
recipe and Phase 137's roadmap doc for the design notes. Phase 140
removed the legacy `find_package(NanoRos CONFIG)` / `just install-local`
flow — `add_subdirectory` is the only supported shape today.

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
`examples/native/c/listener/` to verify end-to-end.

## Source layout

- `CMakeLists.txt` — 20 lines, `add_subdirectory(nano-ros)` consumer.
- `src/main.c` — publisher loop.
- `package.xml` — declares `std_msgs` dependency for `nros_generate_interfaces`.

## Related

- `book/src/getting-started/build-as-subdirectory.md` — the
  user-facing guide for the canonical path.
- `docs/roadmap/phase-137-source-distribution-entry-cmake.md` —
  design notes for the root entry CMake.
- `docs/release/migration-install-local-removal.md` — Phase 140
  migration note for downstream consumers.
- `examples/native/c/listener/` — receiver-side counterpart.
