---
id: 2582
title: "`[image.<id>.knobs.*]` rows are silently ignored on the hand-written `nano_ros_workspace()` CMake road"
status: open
type: bug
area: [cmake, cli]
severity: high
found: 2026-11-14
related: [phase-478, 2564]
---

GitHub: NEWSLabNTU/nano-ros#3866

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)**,
which holds what the island's port meets while building its image. Found on
the Autoware Safety Island port (`actuation_module/CMakeLists.txt`, FreeRTOS
branch, image `freertos-posix`), on main `37232ecff1`. Sibling of
[issue 2564](2564-hand-entry-bringup-image-drops-domain-id.md): the same
hand-written road drops a different image-stated fact.

## What happens

On the hand-written CMake road (`find_package(nano_ros COMPONENTS workspace)`
plus `nano_ros_workspace(SYSTEM <bringup> ...)`, after `nros configure .`),
the bringup's `[image.<id>.knobs.<group>]` rows do not reach the build, and
nothing says so:

```toml
[image.freertos-posix]
board = "freertos-posix"

[image.freertos-posix.knobs.params]
max_parameters = 164

[image.freertos-posix.knobs.executor]
max_cbs = 17
subscription_buffer_size = 16384
```

Configured with `-DNROS_PLATFORM=freertos -DNROS_BOARD=freertos-posix
-DNROS_DEPLOY=freertos-posix`, and again with `-DNROS_IMAGE_SYSTEM_DIR=<bringup>
-DNROS_IMAGE=freertos-posix`. Both builds succeed with the defaults:

```
nros-params/out/nros_params_config.rs:  pub const MAX_PARAMETERS: usize = 32;
nros-node/out/nros_node_config.rs:      pub const MAX_CBS: usize = 8;
                                        pub const DEFAULT_RX_BUF_SIZE: usize = 1024;
```

With the matching `NROS_*` exports in the build script, the same build has
164 / 16 / 16384.

## Why it matters

The controller declares 149 parameters, so the image states 164 slots. With
32, `declare_parameter<T>()` returns the compiled default for every parameter
past the 32nd and logs nothing: the image builds and boots with wrong
parameters. The book's "Per-image build knobs"
(`book/src/guide/build/configure-an-image.md`) documents the rows without
saying they are read only by `nros build`.

## Expected

Either the rows reach the build on this road (the configure hook already
knows the image), or the configure refuses an image that states knob rows it
will not apply, naming `nros build` as the road that reads them. A silent
default is the one outcome that is not acceptable: it is the "a knob reaches
one road and not another" class CLAUDE.md records for the Zephyr Rust lane,
here on the hand-written CMake road.

## Workaround

Keep the `NROS_*` exports in the build script.
