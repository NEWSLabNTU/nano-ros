---
id: 123
title: "`workspace-shadowing` template-smoke read the sizes-header `#error` stub — the mirror target was never built for a transitive-only consumer"
status: resolved
type: bug
area: cmake
related: [0088, 0090, 0114, 0122]
resolved_in: "nros-{c,cpp}/CMakeLists.txt — static lib depends on its own config-header mirror"
---

## Resolution

The `shadowing` cell of the template compile-check (`workspace-shadowing`) failed compiling its
`consumer` exe (a verbatim rclcpp target instantiating `nros::Publisher<std_msgs::msg::Marker>`)
with `*_OPAQUE_U64S undeclared` / `Publisher<M> has no member storage_` — the per-build sizes header
resolved to the in-tree `#error` stub.

**Root cause (not what the first triage guessed).** It is the 0088-family mechanism, but the failure
was *not* build-order timing and *not* include-path order:

- `nros_c-static` / `nros-cpp-headers` already list the **mirror** dir
  (`${CMAKE_CURRENT_BINARY_DIR}/include`, into which the `nros_{c,cpp}_config_header` custom targets
  copy the real `nros/nros_config_generated.h`) ahead of the source `#error` stub.
- But those mirror custom targets only build **if some target depends on them**. The message-lib /
  entry / carrier paths wire that dep explicitly (issues 0114 / 0090); a plain rclcpp/ament consumer
  that pulls nros-cpp only *transitively* (through the `std_msgs__nano_ros_cpp` binding +
  `NanoRos::NanoRosCpp`) never does. So under `make all` the mirror never ran — its `include/nros/`
  dir stayed empty — and `#include "nros/nros_config_generated.h"` fell through to the source stub.
- Proof: manually `cmake --build <shadowing> --target nros_c_config_header nros_cpp_config_header`
  populated the mirror dir, after which `consumer` compiled + linked clean.

**Fix.** Make the real static libs depend on their own mirror targets, so ANY consumer that links
nano-ros (directly or transitively) builds the per-build headers before compiling:

- `packages/core/nros-c/CMakeLists.txt` — `add_dependencies(nros_c-static nros_c_config_header)`.
- `packages/core/nros-cpp/CMakeLists.txt` — `add_dependencies(nros_cpp-static nros_cpp_config_header)`
  (the cpp mirror copies BOTH the cpp and the c sizes headers into the prepended cpp include dir).

Both guarded by `if(TARGET …)` and scoped to the Corrosion mirror block (absent on zephyr / the
freertos carrier, which generate the header via other paths). Additive — it only forces a mirror
that should always exist, so the explicit-wiring paths are unaffected. Verified:
`bash scripts/build/compile-check-fixtures.sh` now passes the `shadowing` cell (all templates green).

### Dead ends (recorded so nobody repeats them)

Four `add_dependencies(<consumer> nros_{c,cpp}_config_header)` remedies were tried first and ALL
failed — the INTERFACE binding, its `_gen` codegen target, a `_nros_find_ros_msg_package` directory
DEFER, and `nano_ros_link_rmw(TARGET)`. They failed not because add_dependencies can't gate the
compile, but because the verbatim consumer routes through none of those hooks early enough / the dep
never reached it. Anchoring the dep on the static lib the consumer *actually links* is what works.
