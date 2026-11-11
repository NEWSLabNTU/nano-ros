# `talker` — threadx-linux / c

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: `src/threadx_c_talker` is the program, with its own
`main()`, and `src/threadx_c_talker_bringup` holds the `system.toml` that says what to
build. Each `[image.*]` builds that same `main` once (an owned entry,
RFC-0071 D19).

```bash
cp -r examples/threadx-linux/c/talker ~/my-talker && cd ~/my-talker
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build threadx-linux                 # or threadx-linux-cyclonedds
```

## Run

The zenoh image needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`);
`NROS_LOCATOR` at run time overrides the locator the image states. The binary
is `build/threadx-linux-<rmw>-threadx_c_talker_bringup/cmake/pkg/threadx_c_talker/c_talker`.

## Config

Board, RMW, domain and locator: `src/threadx_c_talker_bringup/system.toml`
(`[image.*]` + `[system]`, RFC-0098 D3/D5). `threadx-linux` is zenoh;
`threadx-linux-cyclonedds` is Cyclone DDS on its own domain. No build command
carries any of them, and `src/threadx_c_talker/CMakeLists.txt` names no board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
