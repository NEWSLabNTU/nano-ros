# nros-platform-posix-c

Native C implementation of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) canonical platform ABI (`<nros/platform.h>`) for POSIX hosts.

Reference port — each of the 39 `nros_platform_*` symbols maps to the closest POSIX primitive (`clock_gettime`, `malloc`, `pthread_*`, `nanosleep`, `sched_yield`, …). Behavioural parity with the Rust `PosixPlatform` impl in [`nros-platform-posix`](../nros-platform-posix); the two share the same canonical ABI and **must not be linked into the same binary** (duplicate symbol definitions).

## Build standalone

```bash
cmake -B build -DCMAKE_PREFIX_PATH=$PWD/../../../build/install
cmake --build build
```

The default include search path points at `../nros-platform-cffi/include` (in-tree checkout). Override via:

```bash
cmake -B build -DNROS_PLATFORM_CFFI_INCLUDE=/path/to/include
```

Produces `libnros_platform_posix.a`. Link it into a binary that expects the canonical platform ABI.

## Build via Cargo (integration tests)

The sibling [`nros-platform-cffi`](../nros-platform-cffi) crate's `posix-c-port` Cargo feature compiles this same source file through `cc` and runs an integration test that exercises every dispatch through `CffiPlatform`:

```bash
cargo test -p nros-platform-cffi --features posix-c-port --test c_port_posix
```

## License

Apache-2.0 or MIT at your option. Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
