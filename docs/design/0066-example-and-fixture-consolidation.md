---
rfc: 0066
title: "Example and fixture consolidation: workspaces carry coverage, fixtures carry configuration"
status: Draft
since: 2026-08
last-reviewed: 2026-08-02
implements-tracked-by: [phase-331]
supersedes: []
superseded-by: null
---

# RFC-0066 — Example and fixture consolidation

## Summary

The example tree has two populations that have grown into each other's jobs.
**Feature coverage is expressed as directories** — 28 themed micro-workspaces,
one per (feature × language) — while **build configuration is barely expressed
at all**: 84 of 86 workspace fixture rows are zenoh.

This RFC inverts both:

- a **feature** (QoS, parameters, lifecycle, custom messages, remapping) is a
  **node package inside a large workspace**, not a workspace of its own;
- a **configuration** (RMW, feature set) is a **fixture axis over that
  workspace**, not a directory.

Net: 32 workspace directories become 11, and the RMW axis reaches workspaces
for the first time.

## The measurements this rests on

Taken 2026-08-02 against `examples/fixtures.toml` (337 rows) and the example
tree.

**Themed workspaces carry no build configuration.** Of the 42 fixture rows
belonging to themed workspaces, **34 declare no `features`, no `cmake_defs`
and no `env`**. The rest carry only platform/board defs (already the platform
axis) plus two env tweaks (`NROS_EXECUTOR_MAX_CBS = 8`; realtime's toolchain
pin). A themed workspace is therefore not a configuration variant — it is a
directory that exists to hold one different node package.

**The large workspaces are already the target shape.** `examples/workspaces/
{rust,c,cpp,mixed}` each hold talker/listener, service client/server, action
client/server, a bringup, per-platform entry packages, and `robot1`/`robot2`
entries for multi-node orchestration. They are 41 of 86 workspace rows and span
six platforms; the 28 themed workspaces are 45 rows and are almost entirely
`native` + `zenoh`.

**The build cost is native, not exotic.** By board tier (RFC-0064's registry):
tier 1 is 278 of 337 rows (82 %), tier 2 is 51 (15 %), tier 3 is 8 (2 %).
Consolidating examples is where fixture time is; extracting low-tier boards is
a maintenance-surface question, addressed separately.

**Duplication is concentrated.** Native has 180 rows over 97 distinct example
directories; 37 of those directories are built 2–8 times as variants,
accounting for 120 rows.

## Design

### Features fold into the large workspaces

Each of `workspaces/{rust,c,cpp,mixed}` gains the node packages its themed
workspaces held:

| folded in | from | note |
|---|---|---|
| `qos_talker_pkg`, `qos_listener_pkg` | `ws-qos-*` | |
| `param_talker_pkg` | `ws-params-*` | |
| `lifecycle_talker_pkg` | `ws-lifecycle-*` | |
| `custom_msgs/`, `reading_{talker,listener}_pkg` | `ws-custom-msg-*` | adds custom **interface-package codegen**, which the large workspaces do not exercise today |
| `remap_talker_pkg` | `ws-remap-rust` | |
| `managed_bringup` | `ws-lifecycle-cpp` | a **second system model** in one workspace — this is what exercises orchestration |

Eighteen directories are deleted: `ws-qos-{c,cpp,rust,mixed}`,
`ws-params-{c,cpp,rust}`, `ws-lifecycle-{c,cpp,rust}`,
`ws-custom-msg-{c,cpp,rust,mixed}`, `ws-remap-rust`, `ws-launch-rust`.

### Behavioural outliers stay separate

Folding is for node-API themes. A theme that carries its own **build or process
behaviour** stays a workspace, because folding it would recreate a
configuration axis inside a directory:

| kept | reason it cannot fold |
|---|---|
| `ws-safety-{c,cpp,rust}` | cross-process talker/listener pair with its own e2e semantics; `safety-e2e` is a build feature that changes probed ABI sizes |
| `ws-realtime-{c,cpp,rust}` (+ its `rclcpp`/`subnode`/`subnode-portable`/`mps2` variants) | own toolchain pin (`nightly-2026-04-11`); the only multi-platform theme (native, nuttx, nuttx-riscv, freertos) |
| `ws-sizing-rust` | its value **is** that the launch names zero callback entities while the runtime needs six (issue 0257). Folding into a many-node workspace destroys the property under test |
| `ws-bridge-rust`, `ws-bridge-xrce-rust` | the only non-zenoh workspace rows in the tree; bridge topology, not node API |

### Configuration becomes an axis

Workspace fixtures are declared as a product, not as hand-written rows:

```
workspace fixture = (large workspace) × (rmw) × (feature set)

  workspaces/c     × {zenoh, cyclonedds, xrce} × {default}
  workspaces/cpp   × {zenoh, cyclonedds, xrce} × {default}
  workspaces/rust  × {zenoh, cyclonedds, xrce} × {default, no-liveliness}
  workspaces/mixed × {zenoh}                   × {default}
```

This is the coverage the current tree lacks: workspaces are 84/86 zenoh today,
so the RMW seam is exercised almost exclusively by single-node micro-examples.

### PX4 / uORB is excluded, explicitly

uORB is not another value on the RMW axis and **must not be swept into the
product above**. Per RFC-0011 it is raw `#[repr(C)]` memcpy with no CDR
serialization, an in-process ringbuffer, static discovery, and it **models
neither services nor actions** — the large workspaces contain service and
action packages by construction, so a `× uorb` cell is not merely expensive but
unbuildable.

Two further properties keep PX4 outside this RFC's scope entirely:

- `PlatformId::Px4` is a **CarveOut** in `matrix::CELLS` (no CI runner builds
  SITL), and `examples/fixtures.toml` has **zero `platform = "px4"` rows** — so
  PX4 contributes nothing to the fixture build time this RFC reduces.
- `examples/px4/cpp/firmware/src/modules/nros_uorb_demo` is a **PX4 firmware
  module**, not a standalone project. Under RFC-0064 that is an
  ecosystem-integration case (row 1, a shell), so it is exempt from the
  `platform/lang/example` convention rather than in violation of it.

Phase-325 (uORB interop and bridge) owns that surface. This RFC changes nothing
under `examples/px4/` and adds no uORB axis value.

### Standalone examples

Already conformant. `platform/lang/example` holds for the six real platforms
(`native`, `qemu-arm-freertos`, `qemu-arm-nuttx`, `threadx-linux`, `zephyr`,
plus the bare-metal trees). Only three trees deviate and each is a separate,
cheap follow-up rather than part of this work: `bridges/` (no language level),
`templates/` (copy-out scaffolds — a different kind of artifact), and the
partial-language trees (`px4`, `stm32f4`, `qemu-esp32-baremetal`).

## Relationship to adjacent work

- **Phase-329 (test taxonomy completion)** moves *tests* onto the generated
  matrix. This RFC changes *which artifacts exist* for those tests to bind to.
  They are complementary and must land in a known order: consolidation changes
  the cell set, so phase-329's `matrix_fixture_coverage` gates are the check
  that no cell is silently dropped here.
- **RFC-0051 / RFC-0061** own the cell taxonomy and the tier ladder. This RFC
  does not change either; it reduces the artifact count each cell builds.
- **RFC-0063 / phase-330 (system model as a build artifact)** touches bringup
  generation. `managed_bringup` folding in means one workspace carries two
  models, which that work should be aware of.
- **RFC-0064 (board support organization)** covers the board-tier extraction
  that motivated this campaign. Deliberately out of scope here — the
  measurements above show it is a maintenance-surface win, not a build-time
  one.

## Cost this does not hide

Each large workspace grows from roughly 8 to 13 node packages, so its
individual fixture build gets slower even as the total drops. The win is 18
fewer `nros sync` + CMake-configure cycles against four slightly larger builds.
**This has not been measured.** Phase-331 W1 measures it before the fold, so
the trade is a number rather than an assertion.

Second cost, accepted knowingly: a QoS regression now fails inside a workspace
that also builds pubsub, service and action packages. Bisection is coarser and
one broken node package blocks that workspace's whole fixture. Option (c) of
the brainstorm — splitting each language into a "core" and a "features"
workspace — trades this back at the price of doubling the workspace count, and
was rejected because the duplication being removed is exactly the near-identical
talker/listener triplets.

## Open questions

- Does folding `custom_msgs` into four workspaces create interface-package name
  collisions across them, or does each keep its own local package? (Local, on
  current reading; confirm during W2.)
- `ws-launch-rust` is folded on the assumption that launch handling is
  exercised by the large workspaces' bringups. Verify before deleting it.
