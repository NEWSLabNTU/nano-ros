C++ API: the headers moved from `nros/` into `rclcpp/` (phase-503, the C++
half of phase-501). `<rclcpp/rclcpp.hpp>` is now the umbrella itself rather
than a forwarder. Headers with an upstream counterpart take upstream's path
(`<rclcpp/node.hpp>`, `<rclcpp/logging.hpp>`, `<rclcpp_action/client.hpp>`,
`<rclcpp_action/server.hpp>`, `<rclcpp_lifecycle/lifecycle_node.hpp>`); the
nano-ros-only ones become `<rclcpp/<name>.hpp>` (`<rclcpp/heap_string.hpp>`,
`<rclcpp/polling_subscription.hpp>`, ...). Entry glue, generated-code support
and the cbindgen header `nros_cpp_ffi.h` moved to `<rclcpp/detail/*>`, which
is internal. No forwarding headers remain, so an `#include <nros/…hpp>` no
longer compiles: include `<rclcpp/rclcpp.hpp>`. No identifier changed. The
codegen version is now 18, with minimum 18, because generated C++ message
headers include the new paths: re-run `nros sync` (or rebuild, which
regenerates) for any generated tree from an earlier release.
