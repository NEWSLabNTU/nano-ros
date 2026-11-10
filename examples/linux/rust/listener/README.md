# `listener` — linux / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: the node package is `src/native_listener`, and
`src/native_listener_bringup` holds the `system.toml` that says what to build.
`nros build` generates the entry that boots the node.

```bash
cp -r examples/linux/rust/listener ~/my-listener && cd ~/my-listener
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build native_listener_bringup:linux   # or linux_xrce, linux_cyclonedds, linux_tls
```

Each `[image.*]` is one build: `linux` is zenoh, and `linux_xrce` and
`linux_cyclonedds` name their RMW, and `linux_tls` turns on the node's `link-tls`
feature. The ROS edition is the
workspace's: set `[system] ros_edition` to build for another.

## Run

```bash
nros run linux    # builds it if stale and starts the zenoh router it needs
```

## Config

Board, RMW, domain and locator: `src/native_listener_bringup/system.toml`
(`[image.*]` + `[system]`, RFC-0098 D3/D5). No build command carries any of
them, and `src/native_listener/Cargo.toml` names no board crate: the generated
entry states the board, its RMW backend and its features (RFC-0121 D6).

What the image KEEPS: `system.contract.yaml` beside the bringup's `system.toml` ([RFC-0100](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0100-rmw-agnostic-sizing-model.md) D3) — the per-endpoint QoS
no probe of the code can answer. `nros configure` joins it onto what this node
registers and writes `build/nros/sizing/<image>.toml`, which each backend sizes
its own pools from; `just mem-report <binary>` is where the difference shows.
Delete the file and the image builds byte-identically to one that never had it.

Write the endpoint's name ABSOLUTELY and exactly as the code registers it. A
contract states RESOLVED names, so a relative spelling or a remapped node is one
the build refuses to match rather than guess at — it names the endpoint and the
image keeps its configured pools.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
