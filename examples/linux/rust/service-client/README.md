# `service-client` — linux / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: the node package is `src/native_service_client`, and
`src/native_service_client_bringup` holds the `system.toml` that says what to build.
`nros build` generates the entry that boots the node.

```bash
cp -r examples/linux/rust/service-client ~/my-service-client && cd ~/my-service-client
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build native_service_client_bringup:linux   # or linux_xrce, linux_cyclonedds
```

Each `[image.*]` is one build: `linux` is zenoh, and `linux_xrce` and
`linux_cyclonedds` name their RMW. The ROS edition is the
workspace's: set `[system] ros_edition` to build for another.

## Run

```bash
nros run linux    # builds it if stale and starts the zenoh router it needs
```

## Config

Board, RMW, domain and locator: `src/native_service_client_bringup/system.toml`
(`[image.*]` + `[system]`, RFC-0098 D3/D5). No build command carries any of
them, and `src/native_service_client/Cargo.toml` names no board crate: the generated
entry states the board, its RMW backend and its features (RFC-0121 D6).

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
