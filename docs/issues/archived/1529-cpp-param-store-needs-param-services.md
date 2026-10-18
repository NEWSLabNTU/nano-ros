---
id: 1529
title: "A C++ image's parameter STORE comes only with `param-services`, so a
  bringup with `features = []` does not link its launch seeds and halts at its
  first `declare_parameter` (-16)"
status: resolved
type: bug
area: [cmake, zephyr, build]
severity: medium
found: 2026-09-28
resolved: 2026-10-18
related: [0745, 1260, 1100, 1272, phase-426, phase-461]
---

**Home: [phase-461](../../roadmap/phase-461-service-inbox-per-family.md)** — homed 2026-10-17; the phase doc already tracks it in its body.

## What this is

The C++ node facades (`rclcpp::Node::declare_parameter`, the node-scoped
`nros_cpp_node_declare_param_*` family `nros::ComponentNode` and ported
nodes call) and the generated entry's launch seeds
(`nros_cpp_declare_param`, emitted for every launch `<param>` whether or
not the bringup serves parameters, by `declare_calls.jinja`'s own rule) are
present in every C++ image. Their store was compiled only under nros-cpp's
`param-services` cargo feature (`params_shim.rs`), which the build turns on
only when the bringup declares `param_services`. So a bringup that turns the
services off, to drop their six queryables per node, loses the store too:

1. **It does not link** when the launch file carries a `<param>`:

       qemu_entry_nros_main_generated.cpp:81:(.text._ZL18__nros_entry_setupv+0x3a): undefined reference to `nros_cpp_declare_param'

2. **It does not boot** when it does not: every `declare_parameter` answers
   `NROS_CPP_RET_UNSUPPORTED` (-16), which `ComponentNode` makes boot-fatal
   through `set_error`.

Found on the Autoware Safety Island (MR-CANHUBK344), whose image fits the
board only with the parameter services off (the six servers per node cost
~95 KB of heap at the first spin and 24 of its 31 queryables). Its workaround
was two local patches: the four launch `<param from>` lines commented out,
and each component's `declare_parameter` helper returning the compiled-in
default on -16.

phase-461 W6 says "the store itself is unconditional in nros-node". On this
main it is not: the declaration API (`ensure_parameter_store`,
`declare_parameter_on`, the getters) is `#[cfg(feature = "param-services")]`
in `executor/spin.rs`, and `ParamState` lives in the `parameter_services`
module beside the six servers. What IS true is that the six servers go up
only when an entry REQUESTS them (`ParamState::requested`, set by
`register_parameter_services`), so the store can run with the service code
linked and dormant.

## Fix (the Zephyr road)

- nros-cpp splits its cargo feature: `param-store` (the store entry points
  in `params_shim.rs`, the launch seed included) and `param-services =
  ["param-store"]` (adds `nros_cpp_register_parameter_services`). Both
  forward `nros-c/param-services`, because nros-node has not split the
  store from the service code.
- `zephyr/CMakeLists.txt` builds every C++ image with `alloc,param-store`,
  and with `param-services` only when `param_services` is declared. The two
  nros-c strings name `nros-c/param-services` in both cases, so the C and
  C++ halves keep one nros-c unit (issue 1100). One variable per crate, set
  once, beside `_nros_trace_suffix`.
- Nothing else moves: the entry calls `nros_cpp_register_parameter_services`
  only for a bringup that declares `param_services`, and the entity inventory
  counts the six queryables per node only for that capability, so a
  `features = []` image carries the store and no parameter queryable.
- `check compile-smoke` compiles nros-cpp with `param-store` alone, the
  shape no lane compiled before.

What such an image has: `declare_parameter` returns the launch value when
the launch file seeds one and the declared default otherwise; `get`/`set`
from the image's own code work. What it does not have: `ros2 param
get|set|list|describe|dump` (no service answers), and the `use_sim_time`
runtime switch (the seed exists; nothing can set it). Cost: the dormant
service code in flash, and the store's heap when a node declares.

## Still open

- **The hosted / workspace road** (`cmake/NanoRosFeatureSet.cmake`) keeps
  the coupling: it adds `param-services` for a declared `param_services` and
  nothing otherwise, and it computes nros-c's and nros-cpp's sets in two
  calls that have to agree. A posix C++ image with `features = []` and a
  launch `<param>` still fails to link there.
- **The nros-node split and the `params` axis** are phase-461 W6. When it
  lands, `param-store` forwards the store feature it adds instead of
  `nros-c/param-services`, and the dormant service code leaves the image.

## Resolution (2026-10-18, phase-461 W8): the hosted road carries the store too

**Second bullet of "Still open": done.** phase-461 W6 added the `params`
axis and issue 1897 split nros-node's `param-store` from `param-services`.

**First bullet: reproduced, then fixed.** A one-image workspace (the
`features` workspace's `cpp_param_talker_pkg` and `cpp_params.launch.xml`,
whose `<param name="publish_period_ms" value="250"/>` the entry seeds, on a
bringup with `features = []`) built with `nros build
demo_bringup:linux_cpp_params` failed exactly as this issue's first symptom
says:

```
linux_cpp_params_entry_nros_main_generated.cpp:(.text+0x100): undefined reference to `nros_cpp_declare_param'
```

`nros_feature_set` (`cmake/NanoRosFeatureSet.cmake`) now adds `param-store`
to every image whose capabilities do not already bring `param-services`, for
both crates whatever `CRATE` is, so nros-c's and nros-cpp's sets still name
one nros-c unit (issue 1100). That is the rule the Zephyr road has applied
since the fix above (`_nros_cpp_param_suffix`), now on the CMake road too.
The services remain a capability: `param_services` is the only thing that
brings them or their queryables.

Measured after, same workspace: the image links, boots against a router on
a private port, and prints `Published: 250` (12 lines in 6 s): the launch
seed reached the store and `get_parameter` read it back. Before the fix the
same image did not link.

Gates rerun: `rtos-feature-set-excludes-analysis`, `capability-conditionals`,
`capability-flavour-guards`, `check-feature-set-ssot`,
`check-nros-c-feature-agreement`, `check-flavour-lanes`,
`check-baremetal-platform-arms`, all OK.
