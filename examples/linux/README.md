# examples/linux — host (Linux) examples

Desktop/host examples in C, C++ and Rust. Just module: **`linux`**
(`just/linux.just`).

## Prerequisites

```sh
source ./activate.sh          # PATH: nros (zenohd stays off PATH; `just linux zenohd` resolves it)
nros setup linux             # host toolchains (zenoh RMW default)
```

No SDK env vars are needed. `nros build` finds the checkout its own `nros`
came from; a plain cmake configure of a C/C++ copy-out names it with
`-Dnano_ros_ROOT=<path>`, `find_package(nano_ros)`'s hint.

## RMW selection

The RMW is part of the deployment, so it is named where the rest of the
deployment is: `[system] rmw = "zenoh" | "xrce" | "cyclonedds"` in the example's
`system.toml`, identically for C, C++ and Rust (RFC-0098 D3/D5). Edit that line
and re-run `nros configure` — there is no Cargo feature and no `-D` flag to pass.

(The `rmw-*` Cargo features still exist in the Rust manifests, and the few
leaves that have not gained a `system.toml` yet still resolve their backend
through them. RFC-0098 retires that spelling.)

## Build & run one example

```sh
# terminal 1: the router
ros2 run rmw_zenoh_cpp rmw_zenohd        # zenohd on tcp/127.0.0.1:7447

# terminal 2: any leaf in this tree, same two commands
cd examples/linux/rust/talker
nros configure && nros build
./build/linux/target/debug/talker
```

A C or C++ leaf skips `nros configure` — its message bindings are a CMake-time
output — and builds with its own CMake: `cmake -B build && cmake --build build`.

Contributor shortcuts inside a checkout (they drive cargo and cmake directly):

```sh
just linux zenohd &                     # resolves the installed zenohd
just linux talker                       # = cd examples/linux/rust/talker && cargo run
just linux listener                     # peer in a second shell
just linux build-c                      # builds c/talker + c/custom-msg
just linux build-cpp
```

Contributor test lanes: `just linux test` (Rust), `test-c`, `test-cpp`, `test-rmw`.

## Cases

The listener, service and action pairs are images of the language workspaces
(`examples/workspaces/<lang>/src/{pubsub,service,action}_bringup`, image ids
`linux_<role>[_xrce|_cyclonedds]`, RFC-0122 D1). The leaves left here are the
talker witness, the variants and the feature demos.

| Role | c | cpp | rust |
| --- | --- | --- | --- |
| talker | yes | yes | yes (+`-rtic`, `serial-*`) |
| listener | workspace | workspace | `listener` leaf for the `link-tls` and peer-mode builds (+`-rtic`, `serial-*`) |
| service-server / service-client | workspace (+`-callback` leaf) | workspace (+`-callback` leaf; `service-client` leaf, a hand-written `main`) | workspace (+`-rtic`, `-callback` leaves) |
| action-server / action-client | workspace | workspace (+`-callback` leaf; `action-client` leaf, reads `NROS_TEST_GOAL_ORDER`) | workspace (+`-rtic` leaves) |
| extras | custom-msg, custom-platform, custom-transport-loopback, logging, parameters, safety-listener | logging, parameters, safety-listener | custom-msg, custom-transport-{talker,listener}, lifecycle-node, logging |

(`rust/dds/` is a shared cyclonedds build-support crate, not an example case.)

Coverage authority: [`examples/README.md`](../README.md) coverage matrix.
