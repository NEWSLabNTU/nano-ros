---
id: 1115
title: "The NuttX config header is a COMMITTED snapshot, and phase-429 W1 added the codegen-version macros to the template without adding them there"
status: resolved
type: bug
area: build, testing
severity: high
related: [0167, 0196, 0464, 0834, 0954, 1114, phase-429]
found: 2026-09-06
resolved: 2026-09-07
---

# The guard and the thing it checks are produced by different authors — and on NuttX one of them is a human

## Symptom

`just build-test-fixtures lane=tier2` fails the whole nuttx family, in an
existing build dir:

```
examples/qemu-arm-nuttx/c/talker/build-zenoh/nano_ros_c/builtin_interfaces/msg/
builtin_interfaces_msg_time.h:21:2: error: #error "nros: the generated config
header did not define NROS_CODEGEN_VERSION. This generated artifact cannot tell
whether the runtime accepts it; rebuild the nano-ros runtime so its config
header is regenerated."
```

## Cause — MEASURED, and the first diagnosis was wrong in every particular

`828a06616` (2026-09-05, phase-429 W1) added, in ONE commit and correctly
paired, the `NROS_CODEGEN_VERSION` / `_MIN` **defines** to
`packages/api/nros-c/templates/nros_config_generated.h.template` and the
consuming `#error` **guard** to every generated message header
(`packages/cli/rosidl-codegen/packs/_codegen_version.jinja`).

**On NuttX, that template is not what a TU sees.**
`packages/api/nros-c/include/nros/nros_config_generated.h` is a DISPATCHING
STUB: under `-DNROS_PLATFORM_NUTTX` it includes the committed snapshot
`nros_config_generated_nuttx.h` (`nros-cpp` has the identical arrangement).
Those two files are hand-maintained twins of the per-build header — the same
snapshots that had already drifted in #167, #464 and #954 — and phase-429 W1
did not touch them. So the runtime's half of the version check was simply
absent, the `#ifndef` arm fired, and every NuttX C and C++ image stopped
compiling.

Three claims in the first version of this issue were wrong:

* **"the dependency edge is NOT missing — the failure is ORDERING."** There is
  no ordering to get right. The per-build header is on NO NuttX include path:
  the string `nros-c-generated` appears **zero** times in a NuttX leaf's
  `build.ninja` and zero times in its `c_talker_includes.txt`, and the
  preprocessor's own `-M` output for the failing TU names exactly two config
  headers, both in `packages/api/nros-c/include/nros/`.
* **"CI cannot hit this — a fresh clone has no build dirs."** A fresh clone hits
  it every time. The defect is in TRACKED SOURCE. It survived because nothing
  merge-gating builds NuttX, not because it needed local residue.
* **"13 of 13 affected leaves are stale."** True and non-causal. Those stale
  `build-*/cargo-target/nros-c-generated/nros/nros_config_generated.h` files are
  real, and nothing reads them.

For the record, the ordering the issue described is real but points the other
way: `nros-nuttx-ffi` (which runs the cargo build that emits the per-build
header) is order-only **after** `libbuiltin_interfaces__nano_ros_c.a`, and the
0088/0114 protections in `NanoRosGenerateInterfaces.cmake` are guarded on
`TARGET nros_c_config_header` / `cargo-build_nros_c`, neither of which exists in
a NuttX build. That is why a committed fallback exists at all.

## The convergence question, answered: NO — and no build-dir operation could have

The issue asked whether an incremental re-run, or the documented
`cmake <build-dir>` escape hatch, repairs the state, and recorded that the
`rm -rf` exemption was not earned. It is still not earned, and it never can be
for this defect:

* **Repro with no build directory at all**, in a pristine worktree —
  a five-line TU, `-DNROS_PLATFORM_NUTTX`, `-I packages/api/nros-c/include`,
  reproduces `NROS_CODEGEN_VERSION` undefined. Nothing in a build tree
  participates.
* **`ninja -t query` on the failing object** shows its order-only deps are its
  own generated sources and nothing else — no config-header edge to be stale.
* **`cmake <build-dir>`** was run: it fails for an unrelated reason on this host
  (a stale in-tree CLI), and would not have helped either — a reconfigure
  regenerates the message headers with the same guard and cannot touch a tracked
  header.
* **`rm -rf` would have "worked" only in the sense of building nothing**: the
  rebuilt tree fails identically. Wiping here would have destroyed the one
  reproduction while changing no outcome — the antipattern in its purest form.

## Fix

* `NROS_CODEGEN_VERSION` / `NROS_CODEGEN_VERSION_MIN` added to
  `packages/api/nros-c/include/nros/nros_config_generated_nuttx.h` and
  `packages/api/nros-cpp/include/nros/nros_cpp_config_generated_nuttx.h`, as
  EXACT mirrors of `nros_core::codegen_version` (a version range has no safe
  upper bound, unlike every size in those files). Verified end to end: the real
  generated `builtin_interfaces_msg_time.h`, re-stamped to the current codegen
  version, compiles against the fixed snapshot; and both fallbacks in one TU
  produce no redefinition diagnostic.
* Gate `check-config-fallback-macros` (fast lane, buildless, self-testing):
  every macro a codegen pack READS must be defined by every reachable committed
  fallback, and the version pair must equal the Rust constants. Both directions
  demonstrated against the real tree — deleting the define and skewing its value
  each turn the gate red.

## What this leaves open

* The NuttX snapshots remain hand-maintained for SIZES, which is the
  #167/#464/#954 line. This gate covers macro NAMES and the one pair that has an
  exact answer; it cannot make a bound current. The structural fix would be to
  get the per-build header onto the NuttX include path before the interface
  library compiles — i.e. give NuttX the `nros_c_config_header` ordering the
  other platforms have — which is a build-graph change, not a header edit.
* `check-gate-visibility` still reports what no merge-gating lane reaches. NuttX
  builds are in that set, which is the reason this took two days to surface.
