---
id: 2582
title: "`[image.<id>.knobs.*]` rows are silently ignored on the hand-written `nano_ros_workspace()` CMake road"
status: resolved
type: bug
area: [cmake, cli]
severity: high
found: 2026-11-14
resolved: 2026-11-15
resolved_in: "the change that resolved it (one change for issues 2564 and 2582)"
related: [phase-478, 2564]
---

GitHub: NEWSLabNTU/nano-ros#3866

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)**,
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

## Resolution

The two issues are one gap: a hand-written CMake configure that NAMES its image
got none of that image's facts, because the one reader that delivers them
(`nano_ros_read_leaf_system`, over `nros dev image leaf-system`) spoke for the
configure only when the configure's top-level directory was a standalone leaf.
Issue 2564 is the domain half and issue 2582 the knob-row half; one change
fixed both.

- `nano_ros_read_leaf_system(DIR <d> IMAGE <id> CLAIM)` reads one image of a
  system.toml that is not the top-level project as the image this configure
  builds: its image env reaches the cargo carrier (`NROS_IMAGE_ENV_CONFIG`),
  and its domain, edition and network are the configure's, under the same
  agreement rules as a standalone leaf.
- `nano_ros_workspace(SYSTEM <bringup>)` claims the image the configure names
  (`-DNROS_IMAGE=<id>`) before its import, which is where the carrier
  attaches. A generated root (`NROS_CONFIGURED_IMAGE`) already sets the
  carrier and is left alone. A root that names no image while its bringup
  states per-image knob or env rows stops the configure, naming
  `-DNROS_IMAGE` and `nros build`.
- `nano_ros_add_executable(… BRINGUP <dir> IMAGE <id>)` with no `DOMAIN_ID`
  asks the reader for that image's domain (`nros_image_domain_id`), and holds
  it to the issue-1946 agreement rule against a `-DNROS_DOMAIN_ID`.

Test: `tests/cmake-hand-written-image-facts-tests.sh`
(`just check hand-written-image-facts`), with a stand-in CLI. It asserts the
domain query, a claimed read and an unclaimed one, the refusal of a
disagreeing -D, and the reach. Mutation-checked: removing CLAIM's effect turns
the claimed-read and refusal cases red.

The book's "Per-image build knobs" now says which roads apply the rows, and
how a hand-written root names its image.
