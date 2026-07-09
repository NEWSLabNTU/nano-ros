# `parameters` — native / cpp

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/native/cpp/parameters ~/my-parameters && cd ~/my-parameters
cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros   # or: export NROS_REPO_DIR=…
cmake --build build
```

## Run

Needs a zenoh router (`just native zenohd` in the nano-ros checkout);
the built binary lands under `build/`.

## Config

Wiring lives in `nano_ros_entry(…)` in `CMakeLists.txt`; select the
backend with `-DNROS_RMW=<backend>`, locator/domain via
`NROS_LOCATOR` / `ROS_DOMAIN_ID`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
