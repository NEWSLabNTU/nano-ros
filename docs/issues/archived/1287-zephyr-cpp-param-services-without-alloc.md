---
id: 1287
title: "A Zephyr C++ image on zenoh with param_services fails to compile: nros-c
  is built with param-services and without alloc"
status: resolved
type: bug
area: build, zephyr
severity: medium
resolved_in: "fix(#1287): the Zephyr C++ feature string spells alloc with param-services"
related: [issue-0730, issue-0745]
---

## What happens

A Zephyr C++ image on the zenoh backend that declares `param_services` stops
at:

```
compile_error!("`param-services` allocates: add \"alloc\" to this crate's features");
```

(`nros-node/src/lib.rs`). Measured on Autoware Safety Island's MR-CANHUBK344
image (zenoh, four C++ component nodes, `param_services` declared): it fails
with this error without the change below and builds with it.

## Why

`zephyr/CMakeLists.txt` builds the C++ feature string per backend. The xrce
and cyclonedds strings spell `alloc`; the zenoh string does not, because
nros-rmw-zenoh carries it transitively (the shape issue 0730 warned about).
The issue 0745 mirror then appends `param-services` when the consumer passes
`param_services` in `NANO_ROS_FEATURES`.

The nros-c library built for C++ does not inherit that transitive `alloc`: it
copies `alloc` and `param-services` into its own feature string only when the
C++ string SPELLS them (`if(_nros_cpp_features MATCHES "(^|,)alloc(,|$)")`).
So on zenoh it got `param-services` and no `alloc`, and nros-node's
requirement check refused.

## Fix

Append `alloc` together with `param-services`, where the capability is
added. That is the phase-361 W8.e rule: a capability names its requirement,
it never relies on something else having switched it on.

The Rust facade's own param-services compile failure (the discarded
`declare_parameter` result) is a different defect, fixed on main by
`eb8111ad1`.
