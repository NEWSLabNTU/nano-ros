# carrier_usage_reqs_ws

Issue 2817. One C++ component, `cur_pkg::Probe`, whose library
(`nano_ros_auto_add_library(cur_lib)`) has a PUBLIC include directory outside
`cur_pkg/include` and a PUBLIC link to `cur_vendor`. The registration is TYPED,
so the linux image's build also builds the native carrier
(`<pkg>__<exe>`, program `cur_probe`). The carrier must take both usage requirements from the
library it links. `tests/carrier_usage_reqs.rs` builds the image and runs the
carrier, which prints `CARRIER_USAGE_REQS answer=42` once.
