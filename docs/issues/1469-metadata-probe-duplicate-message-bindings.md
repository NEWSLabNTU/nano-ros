---
id: 1469
title: "The metadata probe generates a shared message type's bindings once per
  consuming package and then `include!`s every copy into one crate, so any
  workspace with two C++ node packages that share a message dependency fails to
  compile and every node goes `unprobeable`"
status: open
type: bug
area: codegen, cmake, metadata
severity: high
found: 2026-09-24
related: [1467, 1398]
---

## What happens

Every node in the Autoware Safety Island is `unprobeable`, so the workspace
has no source metadata at all. `nros sync` says so once per node and stops
there:

```
sync: source metadata - 0 rebuilt, 0 already current
sync: source metadata - no producer for autoware_mrm_comfortable_stop_operator::mrm_comfortable_stop_operator (probe failed at this source last sync; unchanged)
sync: source metadata - no producer for autoware_mrm_emergency_stop_operator::mrm_emergency_stop_operator (probe failed at this source last sync; unchanged)
sync: source metadata - no producer for autoware_mrm_handler::mrm_handler (probe failed at this source last sync; unchanged)
sync: source metadata - no producer for autoware_stop_mode_operator::stop_mode_operator (probe failed at this source last sync; unchanged)
```

The real error is two layers down, in the probe's own cmake build:

```
error[E0428]: the name `builtin_interfaces_msg_time_t` is defined multiple times
  --> src/../../../autoware_mrm_comfortable_stop_operator/nano_ros_cpp/builtin_interfaces/msg/builtin_interfaces_msg_time_types.rs:19:1
   |
19 | pub struct builtin_interfaces_msg_time_t {
   | ^^^^^^^^^^ `builtin_interfaces_msg_time_t` redefined here
   |
  ::: src/../../../autoware_mrm_handler/nano_ros_cpp/builtin_interfaces/msg/builtin_interfaces_msg_time_types.rs:19:1
   |
19 | pub struct builtin_interfaces_msg_time_t {
   | ---------- previous definition of the type here

error: could not compile `nano-ros-cpp-ffi-nav_msgs` (lib) due to 8 previous errors
gmake[1]: *** [CMakeFiles/Makefile2:398: CMakeFiles/probe_autoware_mrm_handler__mrm_handler.dir/rule] Error 2
```

## Why

`nano_ros_cpp_ffi_nav_msgs/src/lib.rs` is a flat list of 77 `include!`
lines, and the path each one resolves to is a CONSUMING PACKAGE's generated
directory rather than one place per message type:

```
$ grep -c '^include!' .../nros-ws-nav_msgs/nano_ros_cpp_ffi_nav_msgs/src/lib.rs
77
$ grep -oE '\.\./\.\./\.\./[a-z_]+/' .../src/lib.rs | sort | uniq -c
     64 ../../../autoware_mrm_handler/
      2 ../../../autoware_mrm_comfortable_stop_operator/
$ grep -oE '[a-z0-9_]+_types\.rs' .../src/lib.rs | sort | uniq -d
builtin_interfaces_msg_duration_types.rs
builtin_interfaces_msg_time_types.rs
```

`builtin_interfaces/msg/Time` and `builtin_interfaces/msg/Duration` are
generated under BOTH node packages, because both consume them, and the list
is assembled without deduplicating by TYPE. `include!` is textual, so the
crate gets two definitions of every item in those two files. Eight of them
here: the struct, and the serialize/deserialize/teardown functions for each
of the two messages.

The scaling rule is the tell: the failure is proportional to the number of
SHARED types rather than to anything the author wrote.

**Correction (2026-09-24), from the fix in PR #1237.** The sentence that
stood here said "one C++ node package compiles, two that share a message
dependency do not", which reads as a property of a single configure. It is
not, and anyone reproducing it from that sentence alone will fail to. The
generators ARE idempotent within one pass. The duplicate ACCUMULATES ACROSS
configures, because `_NROS_PKG_<pkg>_GENERATED_RS_FILES` is a cmake CACHE
entry that outlives the pass that wrote it, so a build dir re-configured
with a different live package set keeps the earlier package's entry and adds
the new one. The island's own tree shows it: its two copies of
`builtin_interfaces_msg_time_types.rs` carry timestamps from two different
configure passes, not one.

A second correction from the same source: the list IS de-duplicated already
-- by PATH. The paths differ, one per consuming package, which is exactly
why the de-duplication misses. The invariant `include!` needs is
de-duplication by TYPE.

## Not the same as the board build

The real board build gets this right. It generates a shared type ONCE,
under an interface package, and the island's tree shows exactly one copy:

```
build-board/island_interfaces/nano_ros_cpp/builtin_interfaces/msg/builtin_interfaces_msg_time_types.rs
```

against the probe's two:

```
build/nros-metadata/metadata-probe-cmake/build/autoware_mrm_comfortable_stop_operator/nano_ros_cpp/.../builtin_interfaces_msg_time_types.rs
build/nros-metadata/metadata-probe-cmake/build/autoware_mrm_handler/nano_ros_cpp/.../builtin_interfaces_msg_time_types.rs
```

So the defect is in the probe's cmake path only, and the board path already
shows the shape the fix should reach.

## Why it matters now

Source metadata is what phase-463's census reconciles a contract against.
With every node unprobeable the island cannot produce a census, so
`island-W2` (the census as a configure gate) cannot be done at all, and
phase-463 W3's verdicts have nothing to read on the one workspace whose
historical defect motivated them. Two of the four nodes still carry a
`version: 1` sidecar from an older successful probe, recording one node and
one callback each, which is both stale and far short of what those nodes
declare. A stale sidecar that nothing refreshes is worse than none: it reads
as an observation.

## Filed separately: the probe ignores the workspace's field caps

Issue **1470**. With this issue's fix applied the probe gets further and
then stops on a static assertion, because a message the real build bounds
through `nros-codegen.toml` reads as unbounded in the probe. That one is a
design question about where a workspace's caps live rather than a mechanical
duplicate, so it is not folded in here.

## A second, smaller defect: the marker records no reason

The failure is cached as an empty marker beside where the sidecar would go:

```
$ cat src/autoware_mrm_handler/metadata/mrm_handler.json.unprobeable
fnv1a64:c9849d885b4cfb0e:b3a05aa3f5b69bb4
```

Two digests and nothing else. `nros sync` then reports "probe failed at this
source last sync; unchanged" on every later run and does not retry, so the
compiler error above is unreachable from any number of syncs. Recovering it
took deleting the marker by hand. The marker should carry the failure's first
error line, or `sync -v` should print it, or both; a cache entry that
suppresses a diagnosis it does not record costs every future reader the same
excavation.

## Reproduce

```
cd <the safety island workspace>            # four C++ node packages
rm src/autoware_mrm_handler/metadata/mrm_handler.json.unprobeable
nros sync -v                                # E0428, eight times
```

Any workspace with two C++ node packages sharing one message dependency
should do; the island is simply the one at hand.

## Acceptance

- A shared message type's bindings are generated once for the probe, as they
  already are for the board build, and the aggregating crate `include!`s each
  type exactly once.
- The island's four nodes probe, and their sidecars carry the entity counts
  their sources actually declare rather than one callback each.
- A probe failure records its reason where a later reader will find it, and
  `nros sync -v` prints it rather than only that the source is unchanged.
