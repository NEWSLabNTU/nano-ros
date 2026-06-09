---
id: 4
title: nano_ros_generate_interfaces() requires explicit file listing
status: resolved
type: enhancement
area: cmake
related: []
resolved_in: auto-discovery
---

Both the native and Zephyr CMake functions now support auto-discovery when
no files are specified. The C codegen also handles intra-package nested type
dependencies correctly (fully qualified type names, per-type `#include`
directives). Cross-package dependencies must still be declared with
`DEPENDENCIES` and generated separately.
