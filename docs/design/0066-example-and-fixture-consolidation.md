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
  **node package**, not a workspace of its own — all of them collected into ONE
  native-only `features` workspace;
- a **configuration** (RMW, feature set) is a **fixture axis over the large
  workspaces**, not a directory.

Net: 32 workspace directories become 12, and the RMW axis reaches workspaces
for the first time.

> **Revision 2 (2026-08-02).** R1 folded each theme into the same-language large
> workspace (`ws-qos-c` → `workspaces/c`, etc.). Implementing it proved that
> wrong for the two capability-bearing themes: capabilities are an **image**
> property, the large workspaces all contain **embedded** entries, and
> `param_services`/`lifecycle` are alloc-gated features an embedded image must
> opt into explicitly. R2 collects the feature demos into one native-only
> workspace instead. The measurements and the fixture-axis half of R1 are
> unchanged. See *Where a capability applies*.

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

## Where a capability applies — the image, not the workspace

This is the constraint R2 turns on, and the tree already states the intended
unit:

```cmake
cmake/NanoRosFeatureSet.cmake — "---- capabilities ----
                                 Image-level, not platform-level."
```

An **image** — one entry, one executable — is the right unit: a capability
changes what is linked into that binary. "Whole workspace" is too coarse and
"per node package" is meaningless, since a node package is a library and links
nothing.

Two implementations, only one of which achieves it:

| | carrier | actual granularity |
|---|---|---|
| **Rust** | the generated `<entry>_nros_selection` facade | **per entry** |
| **C / C++** | `NANO_ROS_FEATURES` → `nros_feature_set` → one shared `libnros_cpp.a` per configure | **per workspace configure** |

The rust facade is the evidence — the same entry name, two systems:

```toml
# ws-params-rust/generated/nros-selection/native_entry/Cargo.toml
nros = { …, features = ["param-services", "ros-humble"] }
# workspaces/rust/generated/nros-selection/native_showcase_entry/Cargo.toml
nros = { …, features = ["ros-humble"] }
```

**So the rule is already the right one: capabilities are declared on the SYSTEM
an entry consumes.** What differs is the freedom each language has to vary it:

- rust cannot have more than one system per workspace —
  `sync: N bringups declare a system … selection facades are not generated for
  multi-system workspaces (phase-315 W1 models one declaration per workspace)`;
- C/C++ can have several bringups, but every entry in one configure shares the
  single `NANO_ROS_FEATURES` value, so the capability set cannot vary between
  them either. (Worse, each bake `FORCE`-writes that cache var, so with several
  bringups the last one configured silently wins.)

Both are the same limit seen from two sides: **one capability set per
workspace, in practice.**

### Why that rules out folding into the large workspaces

```cmake
# `param_services` / `lifecycle` still imply hosted: both are alloc-gated,
# so an embedded image opts in explicitly rather than getting them by default.
```

Every large workspace contains embedded entries — `rust` has
`esp32/qemu_freertos/qemu_nuttx/threadx_linux/zephyr`; `c`, `cpp` and `mixed`
likewise. With one capability set per workspace, folding a capability-bearing
theme into any of them forces alloc-gated features onto size-constrained
embedded images that the design says must opt in explicitly. `mixed` is no
refuge: it is equally multi-platform.

## Design

### Feature demos collect into one native-only workspace

A new `examples/workspaces/features/` holds every capability demo, in all three
languages, with **no embedded entries**:

```
examples/workspaces/features/
  src/demo_bringup/            ONE system:  [param_services] + [lifecycle]
  src/{c,cpp,rust}_param_talker_pkg
  src/{c,cpp,rust}_lifecycle_talker_pkg
  src/qos_{talker,listener}_pkg          (per language)
  src/custom_msgs/  src/reading_{talker,listener}_pkg
  src/remap_talker_pkg
  src/managed_bringup/                   the manual-transition second system
  src/native_*_entry                     native only
```

| collected | from |
|---|---|
| `qos_{talker,listener}_pkg` | `ws-qos-{c,cpp,rust,mixed}` |
| `param_talker_pkg` | `ws-params-{c,cpp,rust}` |
| `lifecycle_talker_pkg` | `ws-lifecycle-{c,cpp,rust}` |
| `custom_msgs/`, `reading_{talker,listener}_pkg` | `ws-custom-msg-{c,cpp,rust,mixed}` |
| `remap_talker_pkg` | `ws-remap-rust` |
| `managed_bringup` | `ws-lifecycle-cpp` |

Why this shape satisfies every constraint at once:

- **one system** — rust's facade generator is happy;
- **capability union is harmless** — nothing embedded is built here, so the
  alloc-gated `param_services`/`lifecycle` reach only hosted images, which is
  exactly the "opt in explicitly" the platform layer asks for;
- **the large workspaces stay clean** — `{rust,c,cpp,mixed}` keep their
  pubsub/service/action core across six platforms and gain no capabilities;
- **11 themed workspaces become 1**, rather than 3 (one per language).

The cost, stated plainly: this is a **fourth workspace shape**, organised by
*concern* rather than by *language*, sitting alongside `{rust,c,cpp,mixed}`.
That is a real inconsistency in the layout. It is accepted because these
packages exist to demonstrate capabilities, which is a different axis from
"which language binds the API" — and because the alternative violates the
embedded opt-in rule above.

**Correction (W2, 2026-08-02): `managed_bringup` cannot live here.** The
assumption above — that C++-only entries dodge the limit — is wrong. The
one-system rule is per WORKSPACE, not per language: a second bringup makes
`features/` two-system and `nros sync` then refuses selection facades for its
RUST entries, regardless of which language the second system's entries use.

The manual-transition demo therefore became its own workspace,
`ws-managed-cpp`, joining the behavioural outliers. It duplicates
`cpp_lifecycle_talker_pkg`, because that single package carries BOTH
`LifecycleTalker` (autostart) and `ManagedTalker` (manual) and one system cannot
express both. Cost: a 13th workspace and one duplicated package, against this
RFC's stated goal of fewer workspaces. Accepted, because the alternative is
deleting manual-lifecycle coverage.

The split is verified behaviourally rather than by exit code: `features/`
emits `nros_cpp_lifecycle_autostart`, `ws-managed-cpp` emits none while still
linking `nros_cpp_register_lifecycle_services` — a bare `lifecycle` capability
registers the REP-2002 services without driving Configure->Activate.

Seventeen directories are deleted (`ws-launch-rust` is kept — see Open
questions): `ws-qos-{c,cpp,rust,mixed}`, `ws-params-{c,cpp,rust}`,
`ws-lifecycle-{c,cpp,rust}`, `ws-custom-msg-{c,cpp,rust,mixed}`,
`ws-remap-rust`. Net workspace count: 32 → 13 (11 kept + `features` + `ws-managed-cpp`).

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

### The language workspaces are a comparison, so their layout must be parallel

`{rust,c,cpp}` are read side by side — a reader compares how the same system is
expressed in three languages. That only works if the node sets are diffable, and
today they are not: rust names ROLES (`service_server_pkg`) while c/cpp name
PAYLOADS (`add_server_pkg`); prefixing is all (`c_`) / none (rust) / half (cpp,
which has unprefixed `talker_pkg` beside prefixed `cpp_add_client_pkg`); entry
names diverge three ways for the same board.

The rule this RFC adopts:

- **single-language workspace → no prefix.** The directory already names the
  language. Prefixes stay in `mixed` and `features`, where languages coexist and
  the prefix carries information.
- **roles, not payloads.** `service_server_pkg`, not `add_server_pkg` —
  AddTwoInts is what the demo sends, the role is what is being compared.
- **one platform vocabulary** for entries: `freertos` / `nuttx` / `threadx` /
  `zephyr` / `esp32`, no `qemu_` or `native_` qualifier.
- **the same node set in each**, with any exception recorded in the workspace
  README rather than left implicit.

Target: `diff -r workspaces/c/src workspaces/rust/src` shows only per-language
files. phase-331 W2b carries it out, after `features/` exists and before the
deletions, so the renames land once.

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

### Safety is a build axis, not a workspace

`ws-safety-{c,cpp,rust}` looked like behavioural outliers in R2. Reading the
code says otherwise for one half of each:

- the safety **talker** is the plain talker. Its own doc: with
  `NANO_ROS_SAFETY_E2E=ON` "the zenoh backend automatically attaches a CRC-32 +
  sequence number on every publish — **no code change required here**". So it is
  a pure configuration variant and belongs in the feature-set column above —
  where it covers the WHOLE language workspace rather than one pair of nodes;
- the safety **listener** is not. It calls
  `nros_cpp_subscription_register_validated` (surfacing `crc_valid`) where the
  plain listener calls `nros_cpp_subscription_register`. A distinct API surface
  is a capability demo, so it moves into `features/`.

Result: three workspaces become zero, with strictly more coverage.
`safety-e2e` changes probed ABI sizes, so the variant takes its own
`target_dir` (the `target-safety/` precedent).

### Realtime is a dimension, not a feature or a case set

`ws-realtime-*` declares one system — ctrl @10 ms on a high tier, telem @100 ms
on a low tier — and projects it onto each RTOS's native scheduler:

```toml
[[component]] group_tiers = { ctrl = "high" }
[tiers.high]        spin_period = "10000us"
[tiers.high.posix]  priority = 80        # POSIX priority
[tiers.high.zephyr] priority = 5         # RAW Zephyr priority, k_thread per tier
[tiers.high.nuttx]  ...                  # SCHED_FIFO
```

That is the PLATFORM axis applied to scheduling. It follows that the `-mps2` and
`-fvp` splits are simply missing entries: `ws-realtime-c-mps2/src/ctrl_pkg` is
byte-identical to the base and separated only by a `CMAKE_TOOLCHAIN_FILE` block
the language workspaces already carry. 8 workspaces collapse to 3 (phase-331 W6).

Merging the tiers into the language workspaces themselves is deliberately NOT
proposed: the 86 `execution.tiers` dims are the hand-authored data issue 0380
destroyed twice, and re-resolving every realtime model is exactly that hazard.
Revisit once RFC-0063 makes models build artifacts.

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

Under R2 the large workspaces do not grow at all — the feature demos go to
`features/` instead. The trade becomes 17 `nros sync` + CMake-configure cycles
replaced by **one** (plus `features/`'s own build, which is larger than any
single themed workspace but far smaller than the 11 it replaces).

**Measured, 2026-08-03.** Cold `just build-test-fixtures lane=native`, every
manifest-declared build tree wiped first:

| | W1 (`82b82a6d6`, before) | W5 (after) | delta |
| --- | --- | --- | --- |
| wall clock | 7051 s (1 h 57 m) | 6794 s (1 h 53 m) | -257 s (-3.6 %) |
| native stage | 5912 s | 5222 s | -690 s (-11.7 %) |
| fixtures built | 64 | 72 | +8 |
| seconds per fixture | 92.4 | 72.5 | **-21.5 %** |

The per-fixture number is the one that answers this section: the native stage
got 11.7 % faster while building 8 MORE fixtures, because 35 `nros sync` +
CMake-configure cycles became 15. Wall clock understates it — the non-native
remainder grew 433 s for an unrelated reason (a `regenerate-bindings.sh` fix in
the same session made it sync 7 template workspaces it had been silently
skipping, which is new work rather than slower work). Netting that out, the
attributable saving is ~9.8 %.

Caveats recorded rather than smoothed over: W6 landed before W5, so the
realtime/bridge fold is inside this number when the phase doc ordered it
outside; and phase-330/332/333 all moved underneath between the two runs.
Method and full breakdown:
[`docs/roadmap/data/phase-331-w5-remeasure.md`](../roadmap/data/phase-331-w5-remeasure.md),
reproducible via `scripts/dev/measure-fixture-build.sh`.

No regression, so option (c) — a "core" and a "features" workspace per language
— stays unused.

Second cost, accepted knowingly: a QoS regression now fails inside a workspace
that also builds the params, lifecycle and custom-msg demos. Bisection is
coarser and one broken node package blocks `features/`'s whole fixture set. That
is strictly better than R1, where the same break would have blocked a large
workspace carrying pubsub/service/action across six platforms.

Third cost, new in R2 and the reason R1 was written the other way: the layout
gains a **fourth shape**. `{rust,c,cpp,mixed}` are organised by language;
`features/` is organised by concern. A reader looking for "the lifecycle
example" no longer finds it under their language.

## Open questions

- **Should the phase-315 one-system-per-workspace limit be lifted?** It is the
  binding constraint behind R2. Rust cannot generate selection facades for a
  multi-system workspace, and C/C++ `FORCE`-write a single `NANO_ROS_FEATURES`
  per configure, so a workspace has one capability set whichever language it is.
  Lifting it would let a capability travel with its entry — the "image-level"
  granularity `NanoRosFeatureSet.cmake` already claims — and would make R1's
  per-language fold viable after all. RFC-0063 / phase-330 is already reworking
  model generation and is the natural place to consider it. **This RFC does not
  depend on it**; R2 is correct under the limit as it stands.
- **Does the C/C++ `FORCE` last-write-wins need its own fix?** Observed during
  W2: with three bringups, `managed_bringup`'s empty capability set silently
  erased `param_services` + `lifecycle` for the whole cpp workspace. Worked
  around by declaring the union in every bringup. That is a defect in an area
  issue 0353 marked resolved (it fixed the single-bringup path only).
- **Is `mixed` still worth keeping** once the feature demos leave? Its value is
  the language seam (a C++ entry carrying C components). Under R2 it keeps that
  and loses nothing, so yes — but it should be re-examined at W5.
- ~~Does folding `custom_msgs` into four workspaces create interface-package name
  collisions?~~ **Answered (W2, 2026-08-02): no — keep it workspace-local.** All
  four copies are byte-identical (`Reading.msg`, sha `e6ba1fbe0d38`) and all
  declare `<name>custom_msgs</name>`; workspaces build independently and each
  must stay self-contained for copy-out.
- ~~`ws-launch-rust` is folded on the assumption that launch handling is
  exercised by the large workspaces' bringups.~~ **Answered (W2, 2026-08-02):
  the assumption was WRONG — `ws-launch-rust` must be KEPT.** It is the only
  workspace in the tree exercising the launch v1 language surface: `<arg>` with
  defaults, `$(var …)` substitution, `<group ns=…>`, child `<param>`/`<remap>`,
  and `<include>` of a sub-launch with argument pass-through
  (`sensors.launch.xml`). `grep` over `workspaces/rust/src/demo_bringup/launch/`
  finds no `<include>` and no `<group ns=`, so folding it would delete that
  coverage outright. It joins safety / realtime / sizing / bridge as a
  behavioural outlier — the axis it covers is the launch LANGUAGE, not a node
  API, so it cannot become a node package in a large workspace.

  Consequence: the deletion list is **17** directories, not 18.
