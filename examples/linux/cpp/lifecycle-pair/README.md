# `lifecycle-pair` — linux / cpp

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([example directory layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

Two managed nodes in one image, `/lc_alpha` and `/lc_beta`. Each has its
own lifecycle services and state machine, so `ros2 lifecycle` drives one
without moving the other. Each publishes on `/<node>/chatter`, and nothing
goes out until that node is `active`.

## Build

A one-package workspace: `src/cpp_lifecycle_pair` holds the program, and
`src/cpp_lifecycle_pair_bringup` holds the `system.toml` that says what to build. The
program keeps its own `main`, so the image names it as its owned entry
(`entry = "cpp_lifecycle_pair"`) and `nros build` builds it rather than generating one.

```bash
cp -r examples/linux/cpp/lifecycle-pair ~/my-lifecycle-pair && cd ~/my-lifecycle-pair
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build cpp_lifecycle_pair_bringup:linux
```

## Run

```bash
nros run linux
```

Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`). While it spins:

```bash
ros2 lifecycle nodes                       # /lc_alpha and /lc_beta
ros2 topic echo /lc_alpha/chatter std_msgs/msg/String   # silent
ros2 lifecycle set /lc_alpha configure
ros2 lifecycle set /lc_alpha activate      # "lc_alpha: activated", then counts
ros2 lifecycle get /lc_beta                # still unconfigured
```

Each transition prints `<node> on_<transition>`.

## Config

Board, RMW, domain and capability axes: `src/cpp_lifecycle_pair_bringup/system.toml`
(`[image.*]` + `[system]`). No build command carries any of
them.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
