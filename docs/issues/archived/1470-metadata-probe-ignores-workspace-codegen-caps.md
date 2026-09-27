---
id: 1470
title: "The metadata probe never adds the package that supplies a workspace's
  `nros-codegen.toml` field caps, so a message the real build bounds reads as
  unbounded in the probe and the node refuses to compile"
status: resolved
resolved_in: 2026-09-27
type: bug
area: codegen, cmake, metadata
severity: high
found: 2026-09-24
related: [1469]
---

## What happens

The Autoware Safety Island bounds `std_msgs/Header.frame_id`. The board
build honours it. The metadata probe does not, and the node's own source
refuses to compile:

```
.../metadata-probe-cmake/build/nros-ws-autoware_vehicle_msgs/nano_ros_cpp/autoware_vehicle_msgs/msg/autoware_vehicle_msgs_msg_velocity_report.hpp:98:93:
error: static assertion failed: NROS_UNBOUNDED__autoware_vehicle_msgs_msg_velocity_report__field_header_frame_id:
autoware_vehicle_msgs/VelocityReport states no serialized-size bound -- unbounded member: header.frame_id (string).
A bound must EXIST before a buffer can be sized from it.
```

The cap is declared, in `src/island_interfaces/nros-codegen.toml`:

```toml
[fields]
"std_msgs/Header.frame_id" = { cap = 64, mode = "inline" }
```

The same message type therefore generates two headers that disagree, in one
workspace, at the same moment:

| generated header | `NROS_UNBOUNDED` markers | bound |
| --- | --- | --- |
| `build-board/island_interfaces/.../velocity_report.hpp` | 0 | `SERIALIZED_SIZE_MAX = 549`, marked DERIVED |
| `build/nros-metadata/metadata-probe-cmake/.../velocity_report.hpp` | 2 | none |

## Why, and why this is a decision rather than a patch

The island supplies its caps through an explicit
`nros_find_interfaces(CODEGEN_CONFIG ...)` in
`src/island_interfaces/CMakeLists.txt`. The probe never adds that package,
and `nros_workspace_interfaces()` has no workspace-wide notion of a codegen
config to pick it up from. So there is nothing for the probe to read even in
principle: the caps are attached to a package the probe does not build.

That makes this different from 1469, which was a mechanical duplicate the
board build already solved and whose fix reused the existing mechanism. Here
the question is where a workspace's field caps LIVE. Three shapes, none
free:

1. A workspace-wide codegen config the probe and the board build both read.
   Cleanest for the reader, and the largest change, since caps stop being a
   property of whichever package happened to declare them.
2. The probe discovers and adds any package that declares a
   `CODEGEN_CONFIG`. Smaller, but it makes the probe's input set depend on
   package discovery order, and two packages with disagreeing caps become a
   silent race.
3. The probe accepts an explicit caps path from the workspace. Smallest and
   most honest, and it pushes the question onto every workspace author.

This issue does not pick one. It records that the probe and the real build
disagree about whether a type is bounded, which cannot be right under any of
them.

## Why it matters

Field caps are the normal way to bound a ROS message for an embedded
target: an unbounded string has no serialized-size bound, and a buffer
cannot be sized from a bound that does not exist. So this is not an island
quirk. **Any workspace that bounds its messages the usual way cannot be
probed**, and phase-463's census reconciles a contract against exactly the
metadata the probe produces. With 1469 fixed the probe gets further and then
stops here, which is where the island stands today.

## Reproduce

With 1469's fix applied, in a workspace whose messages are bounded through
`nros-codegen.toml`:

```
nros sync -v      # static assertion, NROS_UNBOUNDED__..., in the node's own TU
```

The island is the case at hand; any workspace using caps should do.

## Acceptance

- The probe and the board build agree about whether a given message type is
  bounded, for every type in the workspace.
- The island's four nodes probe, which together with 1469 is what makes a
  census of this workspace possible at all.
- Whichever of the three shapes above is chosen is written down with its
  reason, because the next person to add caps to a workspace will need to
  know where they belong.

## Resolution — 2026-09-27

**The decision is shape 1, and the reason is that it is the only shape that is
order-free.** Field caps live at the WORKSPACE ROOT
(`<ws>/nros-codegen.toml`), which is what RFC-0033 already said — "Workspace
file at the workspace root — shared defaults for all members", discovery
"walking up to the workspace root, deep-merging ancestor → descendant". Nothing
new was invented; what was missing was a CMake lane that could reach it.

Shape 2 (discover any package that declares a `CODEGEN_CONFIG`) is ruled out by
a measurement the issue did not have: under `NANO_ROS_GEN_CACHE_DIR` — which
the probe now sets for issue 1469 — a stock package like `std_msgs` is
generated ONCE for the whole project, so caps attached to a consumer would
belong to whichever consumer the configure reached first. An ancestor of every
consumer cannot have that problem. Shape 3 (an explicit path per workspace)
stays available and unchanged: `CODEGEN_CONFIG` is RFC-0033's priority-1
mechanism and still wins over everything discovered.

### What the two lanes were actually asking

The Rust lane discovers from the package's SOURCE directory
(`cargo_nano_ros::generate_from_package_xml`, `manifest_dir`), so it walks
`<ws>/src/<pkg>` → `<ws>` and finds the workspace file. The CMake lane passed
`args.output_dir` — a directory in the BUILD tree — so it walked up through
`build/` and could not reach a workspace-scope config from ANY CMake build,
whatever the build was for. One question, two answers; the probe was simply the
consumer that noticed, because it is the one that compiles the user's sources
against the user's configuration.

So the fix is not probe-specific, and deliberately so: `_nros_codegen_config_
chain(<start_dir> <out_var>)` in `cmake/NanoRosCodegenCore.cmake` walks up from
the SOURCE dir of the package driving the generation, root-most first;
`_nros_write_codegen_args_json` calls it with `CMAKE_CURRENT_SOURCE_DIR` (a
CMake function does not open a directory scope, so that is the calling
package's dir at every call site) and emits `codegen_config_chain`.
`cargo-nano-ros`'s `resolve_caps_for_args` merges, in precedence order: the
output-dir walk (unchanged — a Zephyr output dir can be nested in the consumer's
tree), then the source chain ancestor → descendant, then the explicit
`CODEGEN_CONFIG`. Both `generate_c_from_args_file` and
`generate_cpp_from_args_file` route through it, where they had a copy each.

The probe needed NO change of its own. It adds the real package directories, so
asking from the source side makes it ask the board build's question.

### Freshness, which was broken in the same place

The chain is now in `add_custom_command`'s `DEPENDS` (canonical lane) and in the
`IS_NEWER_THAN` input loop (Zephyr lane), along with an explicit
`CODEGEN_CONFIG` that had never been in either. Without that, editing a cap
re-emits nothing: the args file names the config, and its CONTENT only changes
when a config APPEARS or MOVES, so every generated header kept the previous
bound.

### Measured, on the smallest shape that carries it

`tests/cmake-probe-workspace-caps-tests.sh` (gate `just check
probe-workspace-caps`, build tier) builds a workspace-local `island_msgs/Report`
embedding `std_msgs/Header` — the island's `autoware_vehicle_msgs/VelocityReport`
exactly, an unbounded string inside a STOCK type, so the cap cannot live in a
`.msg` this workspace owns — plus one C++ node package that subscribes to it, so
its TU instantiates `rx_size_bound<Report>` and the poison is evaluated.

Before, with the cap declared at the workspace root, the probe's own build:

```
[FAIL] the capped field STILL reads as unbounded in the probe's own header:
        91:                      "NROS_UNBOUNDED__island_msgs_msg_report__field_header_frame_id: island_msgs/Report states no serialized-size bound -- unbounded member: header.frame_id (string). A bound must EXIST before a buffer can be sized from it: ...
[FAIL] the header states no RX_MAX_SERIALIZED_SIZE -- the cap did not reach codegen
[FAIL] the node's TU failed to compile with the workspace's caps in place:
        211:/tmp/nros-probe-workspace-caps.PEkxyC/build-capped/nros-codegen/nano_ros_cpp/island_msgs/msg/island_msgs_msg_report.hpp:96:93: error: static assertion failed: NROS_UNBOUNDED__island_msgs_msg_report__field_header_frame_id: ...
[FAIL] 3 of 5 checks failed
```

After:

```
=== B. with no caps, an unbounded member is still refused by name ===
[PASS] the header states no bound and carries the poison token
[PASS] and the node's TU refuses to compile, naming NROS_UNBOUNDED__island_msgs_msg_report__field_header_frame_id

=== A. the probe honours the workspace's nros-codegen.toml ===
[PASS] no NROS_UNBOUNDED marker in the probe's generated header
[PASS] and it states a bound: RX_MAX_SERIALIZED_SIZE = 104
[PASS] and the node's own TU compiles

[PASS] all 5 checks passed
```

**Case B is the load-bearing half and runs FIRST.** A genuinely unbounded field
must still be refused, by name, at the point a size is asked for — issue 1015's
shape: a "fix" that bounded everything by default would pass case A and ship
buffers sized from nothing. The two cases differ by exactly one file and use
separate build trees.

The gate COMPILES, because the poison is a `static_assert` inside a class
TEMPLATE and only a compiler can say whether it fires
(`check-c-array-guard-probe`'s argument, issue 1167). Measured 1 m 49 s warm,
nearly all of it the two node-library compiles.

It is on the FAST line anyway, where its sibling `probe-shared-types` already
sits, and the build tier was measured to be the wrong home rather than merely a
slower one: `.config/ungated-gates.txt` admits a gate only if it FAILS in a
pristine worktree, and this one SKIPS there through the ledger for want of the
in-tree CLI. A build-tier placement would also mean no `pull_request` or
`merge_group` job ever runs it — `check-gate-visibility` said exactly that, by
name, the first time this branch ran the fast lane. At -P32 the cost is
absorbed: `api-parity` on the same lane is 217 s, so this gate sits under the
wall time the lane already has.

### What a workspace author has to know

Caps that must apply to every member go at the workspace root. A
`nros-codegen.toml` beside a package's `package.xml` still applies to what that
package generates (phase-403), and still wins over the workspace file — but it
is not a place from which to bound a type another package may generate first.
The Autoware Safety Island's `src/island_interfaces/nros-codegen.toml` should
move to `<ws>/nros-codegen.toml`; its `nros_find_interfaces(CODEGEN_CONFIG ...)`
can stay, and will then agree with the probe instead of out-racing it.

### Not verified here

The island workspace is not present on this host, so "the island's four nodes
probe" is verified only on the reconstructed shape above, which reproduces its
error text verbatim. Nothing about the fix is island-specific.
