# examples/native — host POSIX (Linux) examples

Desktop/host examples in C, C++ and Rust. Just module: **`native`**
(`just/native.just`).

## Prerequisites

```sh
source ./activate.sh          # PATH: nros, zenohd, play_launch_parser
nros setup native             # host toolchains (zenoh RMW default)
```

No SDK env vars needed for Rust. C/C++ copy-outs resolve the nano-ros root via
`-DNANO_ROS_ROOT=<path>` or the `NROS_REPO_DIR` env var.

## RMW selection

- **Rust** — mutually exclusive Cargo features on each example: `rmw-zenoh`
  (default), `rmw-xrce`, `rmw-cyclonedds`. Example:
  `cargo run --no-default-features --features rmw-xrce`.
- **C / C++** — `-DNROS_RMW=<zenoh|xrce|cyclonedds>` (default zenoh).

## Build & run one example

```sh
# Rust talker (zenoh): router + example
just native zenohd &                     # zenohd on tcp/127.0.0.1:7447
just native talker                       # = cd examples/native/rust/talker && cargo run
just native listener                     # peer in a second shell

# C talker
just native build-c                      # builds c/talker + c/listener + c/custom-msg
# C++ set
just native build-cpp
```

Test lanes: `just native test` (Rust), `test-c`, `test-cpp`, `test-rmw`.

## Cases

| Role | c | cpp | rust |
| --- | --- | --- | --- |
| talker / listener | yes | yes | yes (+`-rtic`, `serial-*`) |
| service-server / service-client | yes (+`-callback`) | yes (+`-callback`) | yes (+`-rtic`, `-async`, `-callback`) |
| action-server / action-client | yes | yes (+`-callback`) | yes (+`-rtic`, `-async`) |
| extras | custom-msg, custom-platform, custom-transport-loopback, logging, parameters, safety-listener | logging, parameters, safety-listener, component-poc, component-node-poc, transform-poc | custom-msg, custom-transport-{talker,listener}, lifecycle-node, logging |

(`rust/dds/` is a shared cyclonedds build-support crate, not an example case.)

Coverage authority: [`examples/README.md`](../README.md) coverage matrix.
