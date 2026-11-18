---
id: 2721
title: "In the nightly's per-platform jobs the configure resolves the cpp workspace's Zephyr images where the metadata probe never ran, so their message-bound seeds find no bound table"
status: resolved
resolved_in: "the change that archives this issue"
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

## Fix (2026-11-19)

**Reproduced, locally.** Configure `examples/workspaces/cpp`. Then remove
the probe build's outputs (`nros-metadata.json` and `nros-codegen/`) and keep
its `CMakeCache.txt`. A plain `nros configure` printed "up to date (90
image(s))". `nros configure --force` re-ran sync, which still did not probe:
the tables stayed missing and 20 seeds refused with "no bound table". This is
the CI symptom.

**Cause.** The refresh decides a C/C++ component is current when its
committed sidecar is fresh AND `probe_build_present`. That predicate checked
`CMakeCache.txt`, which says only that a configure STARTED here (issue 2435's
check, one step short of its own rule). A probe build that failed, or whose
`nros-codegen/` was removed, leaves the cache behind, so every committed
sidecar read as current, the probe never ran again, and every C/C++ west seed
found no table. Which earlier step left such a cache in the nightly's
per-platform jobs is not identified. The predicate is wrong for every such
step, so the fix does not depend on it.

**Fix.** `probe_build_present` now requires what the readers read:
- the cache, as before;
- the probe aggregate `nros-metadata.json`;
- a bound table for every package in the members' interface closure.

The closure is computed by `message_bound_knobs::interface_closure`, the
function the seed itself now uses, so the two cannot disagree. An
unresolvable closure is never present, so the probe re-runs. Both callers use
the predicate: the refresh (`cpp_probe_options`), and the configure stamp's
`stale_sidecar_components`. So a plain configure now notices, too.

Measured after the fix, from the broken state above:
- a configure re-ran the probe; all five tables came back, 0 seeds refused
  and 90 were seeded;
- removing the outputs again with the same CLI: a plain configure reported
  the sidecars "not backed by a complete probe build" and re-probed.

`a_fresh_sidecar_without_the_probe_build_is_probed_again` now steps through
four states: none, cache only, cache plus aggregate with no table, and all
present. Only the last is current. On main, "cache only" reads as current.

Issue 2520's `KNOWN_BOUND_TABLE_GAPS` exemption, and the `MissingBoundTable`
kind it keyed on, are deleted: a missing table is an ordinary failure again,
and refuses under `refuse`.

**Question 1, the wider half: answered by the same fix.** In the broken state
above, the forced configure printed 61 "no metadata probe at … composed from
the model alone" lines, one per image of the workspace on every platform:
ThreadX, FreeRTOS, NuttX, baremetal, Linux and Zephyr. Every one of those
images was composed without the probe's floor (issue 1694's under-size
risk), not just the 14 whose seeds refused. After the fix the same state
printed 0 such lines. The per-platform jobs' 2110 lines are this.

**Question 2: answered, no change.** A configure resolves every image of the
workspace, including ones its host cannot build. That is right: the resolve
(stage 3.5) reads only declarations and the probe, both host-independent, so
it composes the same answer on any host, and with this fix the probe's outputs
exist for it to read. The host-dependent step is the image's own configure
(west, a cross toolchain), and that is already skipped where the host cannot
run it ("no Zephyr found"). Skipping the resolve too would buy nothing, and
it would make a resolve's answer depend on which host ran it.

Sweep: `git grep -n 'probe_build_present\|interface_closure\|probe_tables_needed' -- packages/cli`
