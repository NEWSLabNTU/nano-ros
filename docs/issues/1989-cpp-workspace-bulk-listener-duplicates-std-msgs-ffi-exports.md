---
id: 1989
title: "A second std_msgs subscriber package in the cpp workspace gives it two `libnano_ros_cpp_ffi_std_msgs.a`, and the issue 0277 gate counts 90 duplicate `nros_cpp_*` exports"
status: open
type: bug
area: [build, codegen, cpp, testing]
severity: medium
found: 2026-10-20
related: [0277]
---

## Measured

- Run: run-matrix 39380448771 (schedule, 2026-10-20 06:27 UTC, main at
  `22a482fae`).
- Job: 118507538649, `tier 1 (cells)`, step `just ci tier1 run`.
  `Real failures: 4 / 4 total failures`. Three of them are the trace-marker tests filed in #2444. The
  fourth is this one, from the job's `tier1-junit` artifact:

```
nros-tests::interface_subset_linkage interface_archives_carry_no_duplicate_exports
  panicked at packages/testing/nros-tests/tests/interface_subset_linkage.rs:164:5:
  issue 0277 regression — the per-package FFI split is leaking dependency EXPORTS,
  so two archives define the same symbol and a mixed-subset workspace will fail to link:
  nros_cpp_deserialize_std_msgs_msg_bool: defined in
    examples/workspaces/cpp/build/posix-zenoh-linux/cmake/pkg/bulk_listener_pkg/nano_ros_cpp_ffi_std_msgs/target/x86_64-unknown-linux-gnu/nros-minsizerel/libnano_ros_cpp_ffi_std_msgs.a
  AND
    examples/workspaces/cpp/build/posix-zenoh-linux/cmake/pkg/listener_pkg/nano_ros_cpp_ffi_std_msgs/target/x86_64-unknown-linux-gnu/nros-minsizerel/libnano_ros_cpp_ffi_std_msgs.a
```

There are 90 duplicate symbols, all `nros_cpp_*` std_msgs exports, and every
one is owned by exactly the same pair of archives: `bulk_listener_pkg` and
`listener_pkg`.

## What changed

`examples/workspaces/cpp/src/bulk_listener_pkg` arrived with #2435
(`22f9861a6`, phase-441 W6, the fragmented leg), merged 2026-10-20 at
01:05 UTC. Like `listener_pkg`, it declares `<depend>std_msgs</depend>` and
links `std_msgs__nano_ros_cpp`. Before it, the cpp workspace had one
std_msgs subscriber package, and the gate held. This is the first tier-1
cells run on a tree that has the second one.

`talker_pkg` also depends on std_msgs and is not in the duplicate list. So
whatever decides which package owns a message package's FFI archive
separates a publisher from a subscriber. It does not separate two
subscribers.

## What it is NOT

- Not a flake: the archives are build outputs, and the check reads their
  symbol tables.
- Not the trace-marker tests filed in #2444. Those are the other three failures
  in this job.
- Not shown to break a link yet. The two archives sit in different
  packages, and no image built in this run linked both. The gate exists
  because a mixed-subset image that does link both fails with
  multiple-definition errors.

## What would close it

The defect is in either the FFI split or the new package's shape, and the
fix has to pick which:

1. **The per-package split owns each message package's exports ONCE per
   workspace,** whichever packages use it, so a second subscriber to
   `std_msgs` reuses the first one's archive rather than emitting a copy.
   That is issue 0277's own invariant, and this is the first workspace to
   test it with two packages in the same direction.
2. **Or the leg's package is shaped like its siblings.** If two subscribers
   to one message package are meant to share an archive by some other
   route, `bulk_listener_pkg`'s `CMakeLists.txt` uses a different form from
   `listener_pkg`'s (a bare `if(TARGET std_msgs__nano_ros_cpp)` link,
   without the comment block the other two carry). Align it.

Either way, rerun `interface_subset_linkage` against the cpp workspace with
both subscriber packages built.
