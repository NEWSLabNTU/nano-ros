---
id: 2721
title: "In the nightly's per-platform jobs the configure resolves the cpp workspace's Zephyr images where the metadata probe never ran, so their message-bound seeds find no bound table"
status: open
type: bug
severity: high
area: [sizing, codegen, zephyr]
related: [issue-2520, issue-2404]
found: 2026-11-19
---

**Home:** the message-bound seed (`message_bound_knobs::write_west_seed_over`)
and the metadata probe's codegen. Found while measuring issue 2520.

## What happens

A C/C++ west image's message-bound seed is composed from the bound tables the
metadata probe generated, one per package in the image's interface closure
(`<probe codegen>/<pkg>/` + `rosidl_codegen::INVENTORY_JSON_NAME`). For 14
Zephyr images the closure names a package the probe generated no table for,
and the seed is refused:

```
nros build:   message bounds not seeded: the metadata probe generated no bound table for `action_msgs` (…) -- run `nros configure`
```

Running `nros configure` does not help, and the image is built on default
message bounds. Measured from the real fixture builds of 2026-11-18 (nightly
44242045728 and run-matrix 44250365361): 360 such lines for `action_msgs` and
64 for `builtin_interfaces`, on exactly these images:

| image | missing table |
| --- | --- |
| `demo_bringup:zephyr` | `builtin_interfaces` |
| `demo_bringup:zephyr_cyclonedds` | `builtin_interfaces` |
| `service_bringup:zephyr_service_server` | `action_msgs` |
| `service_bringup:zephyr_service_server_xrce` | `action_msgs` |
| `service_bringup:zephyr_service_server_cyclonedds` | `action_msgs` |
| `service_bringup:zephyr_service_client` | `action_msgs` |
| `service_bringup:zephyr_service_client_xrce` | `action_msgs` |
| `service_bringup:zephyr_service_client_cyclonedds` | `action_msgs` |
| `action_bringup:zephyr_action_server` | `action_msgs` |
| `action_bringup:zephyr_action_server_xrce` | `action_msgs` |
| `action_bringup:zephyr_action_server_cyclonedds` | `action_msgs` |
| `action_bringup:zephyr_action_client` | `action_msgs` |
| `action_bringup:zephyr_action_client_xrce` | `action_msgs` |
| `action_bringup:zephyr_action_client_cyclonedds` | `action_msgs` |

## Scoped (2026-11-19)

It is NOT the probe skipping transitive packages. Configured locally, the cpp
workspace's probe (`build/nros-metadata/metadata-probe-cmake/build/nros-codegen/nano_ros_cpp/`)
holds `nros_message_bounds.json` for all five packages in the closure:
`action_msgs`, `builtin_interfaces`, `example_interfaces`, `std_msgs` and
`unique_identifier_msgs`.

All 424 lines come from ONE run, nightly 44242045728, and only from its
per-platform jobs: FreertosMps2 56, FreertosPosix 56, NuttxArm 84,
QemuBaremetal 112, ThreadxLinux 56, ThreadxRiscv64 56. Run-matrix, host-tests
and merge-runtime show none. In those jobs:
- the workspace's metadata probe never ran: the log has 2110 "no metadata
  probe at …/nros-metadata.json (run `nros configure`)" lines;
- the configure still resolves every image of the cpp workspace, the Zephyr
  ones included, on a host where `configure` then reports "no Zephyr found".

So a Zephyr west image's seed is attempted where it cannot be built and where
no probe ran.

Two questions for the fix:

1. Why the probe is absent in those jobs. If it is skipped on purpose, the
   rest of their images are composed "from the model alone" too: the 2110
   lines say exactly that for the threadx images. That would be a sizing gap
   wider than these 14.
2. Whether a configure should resolve images its host cannot build. It
   already skips their configure ("no Zephyr found").

### Lead, not yet reproduced

In the FreertosMps2 job, `nros configure examples/workspaces/cpp` printed
`sync: metadata — 5 bringups …`, and then no probe outcome line at all. The
resolve that followed reported every committed sidecar
(`src/*_pkg/metadata/*.json`) as "stale against its sources", and found no
`nros-metadata.json`. So the step that decides whether to run the probe, and
the reader that judges the sidecars, may disagree about freshness. Locally,
the same configure in a fresh worktree ran the probe (148 s) and wrote the
file. Not yet explained: whether the CI container (`nano-ros-ci:humble`, a
fresh checkout's mtimes) takes the "nothing to refresh" path.
`NROS_PROBE_SHARED_IMAGE_ROOT` was checked and ruled out: it moves only the
probe's image crate, never `nros-metadata.json`.

## Interim

Issue 2520 refuses a "declared but not derivable" seed under
`NROS_PROBE_FAILURE=refuse`. These 14 `(image, package)` pairs are the one
tolerated exception, `resolve::KNOWN_BOUND_TABLE_GAPS`. They warn, naming
this as a known gap, and any other image or package with the same reason
refuses. When this is fixed, delete that table: the images then pass or refuse
on merit.

## Direction

Answer the two questions above from the nightly workflow and the per-platform
lane's recipe. Then either run the probe where images are resolved, or stop
resolving and seeding an image whose host cannot build it. Fix it where the
lane decides, so no other image on such a host is composed without its probe.
