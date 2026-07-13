# `service-server` — zephyr / cpp

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/zephyr/cpp/service-server ~/my-service-server && cd ~/my-service-server
cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros   # or: export NROS_REPO_DIR=…
cmake --build build
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [zephyr README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/zephyr/README.md).

## Config

See `CMakeLists.txt`; select the backend with `-DNROS_RMW=<backend>`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
