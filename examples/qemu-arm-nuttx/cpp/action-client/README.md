# `action-client` — qemu-arm-nuttx / cpp

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/qemu-arm-nuttx/cpp/action-client ~/my-action-client && cd ~/my-action-client
cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros   # or: export NROS_REPO_DIR=…
cmake --build build
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [qemu-arm-nuttx README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-arm-nuttx/README.md).

## Config

Deploy knobs: `nano_ros_deploy(TARGET … RMW … DOMAIN_ID … LOCATOR …)`
in `CMakeLists.txt`; override the backend with `-DNROS_RMW=<backend>`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
