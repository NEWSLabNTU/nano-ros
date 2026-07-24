# nros-rmw-abi

The **pure C header package** that is the single source of truth for the
nano-ros RMW backend contract (RFC-0054): the vtable every RMW backend
fills (`rmw_vtable.h`), the entity/QoS structs (`rmw_entity.h`), event
types (`rmw_event.h`), return codes (`rmw_ret.h`), and the custom
transport ops (`rmw_transport.h`).

- **C/C++ backends** (cyclonedds, uorb, xrce, out-of-tree) include these
  headers directly (`NanoRos::RmwAbi` CMake target).
- **Rust** (`packages/core/nros-rmw-cffi`) consumes COMMITTED bindgen
  output generated from these headers — regenerate with
  `scripts/gen-abi-bindings.sh` after any header edit. Never hand-edit
  the mirror side; there isn't one anymore.

Any ABI change happens HERE first. Doc comments in these headers are the
canonical docs (they flow into rustdoc via bindgen).
