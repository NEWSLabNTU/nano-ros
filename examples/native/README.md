# examples/native — host native (Linux) examples

Desktop/host examples in C, C++ and Rust. Just module: **`native`**
(`just/native.just`).

## Prerequisites

```sh
source ./activate.sh          # PATH: nros, play_launch_parser (zenohd stays off PATH; `just native zenohd` resolves it)
nros setup native             # host toolchains (zenoh RMW default)
```

No SDK env vars needed for Rust. C/C++ copy-outs resolve the nano-ros root via
`-DNANO_ROS_ROOT=<path>` or the `NROS_REPO_DIR` env var.

## RMW selection

The RMW is part of the deployment, so it is named where the rest of the
deployment is: `[system] rmw = "zenoh" | "xrce" | "cyclonedds"` in the example's
`system.toml`, identically for C, C++ and Rust (RFC-0098 D3/D5). Edit that line
and re-run `nros sync` — there is no Cargo feature and no `-D` flag to pass.

(The `rmw-*` Cargo features still exist in the Rust manifests, and the few
leaves that have not gained a `system.toml` yet still resolve their backend
through them. RFC-0098 retires that spelling.)

## Build & run one example

```sh
# terminal 1: the router
ros2 run rmw_zenoh_cpp rmw_zenohd        # zenohd on tcp/127.0.0.1:7447

# terminal 2: any leaf in this tree, same two commands
cd examples/native/rust/talker
nros sync && nros build
./build/native/target/debug/talker
```

A C or C++ leaf skips `nros sync` — its message bindings are a CMake-time
output — and builds with its own CMake: `cmake -B build && cmake --build build`.

Contributor shortcuts inside a checkout (they drive cargo and cmake directly):

```sh
just native zenohd &                     # resolves the installed zenohd
just native talker                       # = cd examples/native/rust/talker && cargo run
just native listener                     # peer in a second shell
just native build-c                      # builds c/talker + c/listener + c/custom-msg
just native build-cpp
```

Contributor test lanes: `just native test` (Rust), `test-c`, `test-cpp`, `test-rmw`.

## Cases

| Role | c | cpp | rust |
| --- | --- | --- | --- |
| talker / listener | yes | yes | yes (+`-rtic`, `serial-*`) |
| service-server / service-client | yes (+`-callback`) | yes (+`-callback`) | yes (+`-rtic`, `-async`, `-callback`) |
| action-server / action-client | yes | yes (+`-callback`) | yes (+`-rtic`, `-async`) |
| extras | custom-msg, custom-platform, custom-transport-loopback, logging, parameters, safety-listener | logging, parameters, safety-listener, component-poc, component-node-poc, transform-poc | custom-msg, custom-transport-{talker,listener}, lifecycle-node, logging |

(`rust/dds/` is a shared cyclonedds build-support crate, not an example case.)

Coverage authority: [`examples/README.md`](../README.md) coverage matrix.
