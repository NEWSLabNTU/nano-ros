# `service-client` — linux / cpp

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A one-package workspace: `src/cpp_service_client` holds the program, and
`src/cpp_service_client_bringup` holds the `system.toml` that says what to build. The
program keeps its own `main`, which is what this example demonstrates, so
every image names it as its owned entry (`entry = "cpp_service_client"`) and `nros build`
builds it rather than generating one.

```bash
cp -r examples/linux/cpp/service-client ~/my-service-client && cd ~/my-service-client
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build cpp_service_client_bringup:linux
```

No `nros configure` here: a C/C++ package's message bindings are a CMake-time
output.

## Run

```bash
nros run linux
```

Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`).

## Config

Board, RMW, domain and capability axes: `src/cpp_service_client_bringup/system.toml`
(`[image.*]` + `[system]`, RFC-0098 D3/D5). No build command carries any of
them.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
