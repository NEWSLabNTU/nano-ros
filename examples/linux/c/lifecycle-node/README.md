# `lifecycle-node` — linux / c

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([example directory layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A one-package workspace: `src/c_lifecycle_node` holds the program, and
`src/c_lifecycle_node_bringup` holds the `system.toml` that says what to build. The
program keeps its own `main`, which binds one node and registers the lifecycle
services on it, so every image names it as its owned entry
(`entry = "c_lifecycle_node"`) and `nros build` builds it rather than
generating one.

```bash
cp -r examples/linux/c/lifecycle-node ~/my-lifecycle-node && cd ~/my-lifecycle-node
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build c_lifecycle_node_bringup:linux   # or linux_cyclonedds
```

## Run

```bash
nros run linux
```

Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`). While it spins,
`ros2 lifecycle get /c_lifecycle_node` answers `unconfigured`, and
`ros2 lifecycle set /c_lifecycle_node configure` moves it on; each
transition prints its callback's line.

## Config

Board, RMW, domain and capability axes: `src/c_lifecycle_node_bringup/system.toml`
(`[image.*]` + `[system]`). No build command carries any of
them.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
