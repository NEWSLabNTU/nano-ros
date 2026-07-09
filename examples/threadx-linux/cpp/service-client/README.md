# `service-client` — threadx-linux / cpp

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/threadx-linux/cpp/service-client ~/my-service-client && cd ~/my-service-client
cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros   # or: export NROS_REPO_DIR=…
cmake --build build
```

## Run

Needs a zenoh router (`just native zenohd` in the nano-ros checkout);
the built binary lands under `build/`.

## Config

Deploy knobs: `nano_ros_deploy(TARGET … RMW … DOMAIN_ID … LOCATOR …)`
in `CMakeLists.txt`; override the backend with `-DNROS_RMW=<backend>`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
