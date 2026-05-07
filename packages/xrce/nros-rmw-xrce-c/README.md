# nros-rmw-xrce-c

Phase 115.K.2 — micro-XRCE-DDS-Client RMW backend for nano-ros, in C.

This is the C-native re-implementation of `nros-rmw-xrce` (Rust over
`xrce-sys`). It consumes the canonical `nros_rmw_vtable_t` C ABI
defined in `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`
and registers itself via `nros_rmw_cffi_register()`.

Target architecture mirrors `packages/dds/nros-rmw-cyclonedds`:
a static library + a single public header carrying the
`nros_rmw_xrce_register()` entry point.

## Status

**115.K.2.0 (this commit) — vtable scaffold.** Every vtable entry
returns `NROS_RMW_RET_UNSUPPORTED`. The `register` entry point hands
a fully-populated `nros_rmw_vtable_t` to the runtime, but no actual
`uxr_*` calls happen yet. This is the "wired-but-inert" milestone
that 117.3 used as the starting point for Cyclone DDS.

Subsequent sub-phases:

- **115.K.2.1 — session lifecycle.** Wire `xrce_session_open` to
  `uxr_init_*_transport` + `uxr_create_session`, drive_io to
  `uxr_run_session_until_*`, close to `uxr_close_session`. Mirror
  the existing Rust `XrceSession` state machine.
- **115.K.2.2 — pub/sub.** Topic / writer / reader creation. Reuse
  the existing `nros-rmw-xrce` request-id management pattern.
- **115.K.2.3 — services.** Mirror the Rust service paths.
- **115.K.2.4 — Phase 115.E custom-transport bridge.** Port the
  existing `init_transport_from_custom_ops` Rust helper to a C TU
  in this crate so consumers can drain the runtime's
  `NrosTransportOps` slot into `uxr_set_custom_transport_callbacks`.
- **115.K.2.5 — drop the Rust crate.** Once feature parity lands,
  remove `nros-rmw-xrce` and `xrce-sys` from the workspace; the
  CMake `-DNROS_C_RMW=xrce` selector switches over.

See `docs/roadmap/phase-115-runtime-transport-vtable.md` Appendix D
§D.4 for the planned per-file split and §D.5 for risks.

## Building (scaffold)

```bash
mkdir -p build && cd build
cmake -DNROS_RMW_CFFI_DIR=/path/to/packages/core/nros-rmw-cffi/include ..
make
ctest --output-on-failure
```

The scaffold does NOT link against the micro-XRCE-DDS-Client static
library — the stubs never call `uxr_*`. The `MICROXRCEDDS_CLIENT_DIR`
include path exists for forward-compat with 115.K.2.1+, where the
session / pub / sub TUs will start `#include`-ing
`<uxr/client/client.h>`.

## Why C, not C++?

`micro-XRCE-DDS-Client` is C99. The micro-ROS reference impl is C.
Staying in C means no `extern "C"` wrapping, no exception/RTTI knobs,
and the same toolchain baseline as the rest of the XRCE ecosystem.
Cyclone DDS picked C++ because Cyclone's idiomatic API is C++; the
host-language rule (`book/src/internals/rmw-backends.md`) says match
the underlying library, which lands us in C here.
