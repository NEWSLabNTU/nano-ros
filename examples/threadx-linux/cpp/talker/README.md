# `talker` — threadx-linux / cpp

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: the node package is `src/threadx_linux_cpp_talker`
(a component, `threadx_linux_cpp_talker::Talker`, with no `main()`), and
`src/threadx_linux_cpp_talker_bringup` holds the `system.toml` that says what
to build. `nros build` generates the entry that boots the node.

```bash
cp -r examples/threadx-linux/cpp/talker ~/my-talker && cd ~/my-talker
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build threadx-linux                 # or threadx-linux-cyclonedds
```

## Run

The zenoh image needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`);
`NROS_LOCATOR` at run time overrides the locator the image states.
`nros build` prints the binary's path.

## Config

Board, RMW, domain and locator: `src/threadx_linux_cpp_talker_bringup/system.toml`
(`[image.*]` + `[system]`, RFC-0098 D3/D5). `threadx-linux` is zenoh;
`threadx-linux-cyclonedds` is Cyclone DDS on its own domain. No build command
carries any of them, and the node package names no board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
