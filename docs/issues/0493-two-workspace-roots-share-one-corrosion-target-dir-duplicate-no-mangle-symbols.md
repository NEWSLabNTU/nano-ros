---
id: 493
title: "Two cargo workspace ROOTS share one corrosion target dir, so a mixed
  workspace's umbrella staticlib bundles the nros stack twice and the link dies
  on duplicate `#[no_mangle]` symbols"
status: open
type: bug
area: build
related: [phase-340, phase-344, issue-0492, rfc-0070]
---

## Symptom

`just build-test-fixtures lane=native` reaches the workspace fixtures and dies
on `examples/workspaces/mixed` (`ws-group-10`):

```
ld.lld: error: duplicate symbol: nros_rmw_cffi_register
>>> defined at lib.rs:731 (packages/rmw/cffi/src/lib.rs:731)
>>>   nros_rmw_cffi-df6cce59090cd17a.….rcgu.o in archive libnros_ws_runtime.a
>>> defined at lib.rs:731 (src/lib.rs:731)
>>>   nros_rmw_cffi-a49bb61a295363bc.….rcgu.o in archive libnros_ws_runtime.a
```

…and the same for `nros_rmw_cffi_lookup`, `_register_named`,
`_registered_names`, `_set_custom_transport`.

## What is actually duplicated

**One archive, two whole compilations of the nros stack.**
`libnros_ws_runtime.a` (44 MB) contains two `-C metadata` identities of each of
**ten** crates:

```
atomic_waker  log  nros_core  nros_log  nros_node  nros_params
nros_platform_api  nros_platform_cffi  nros_rmw  nros_rmw_cffi
```

For `nros_rmw_cffi`: 9 objects under `df6cce59090cd17a`, 9 under
`a49bb61a295363bc`.

Both rlibs sit in the same `deps/` and were built **41 s apart in one run**
(13:20:57 and 13:21:38) — so this is not stale accumulation, and wiping the tree
does not fix it.

## Cause

The two debuginfo paths name the mechanism:

| identity | debuginfo path | means |
| --- | --- | --- |
| `df6cce5…` | `packages/rmw/cffi/src/lib.rs` | compiled with the **repo root** as workspace root |
| `a49bb61…` | `src/lib.rs` | compiled as a path dep of a **different** workspace |

Two cargo invocations, two workspace ROOTS, **one `--target-dir`**:

1. `packages/api/nros-cpp/CMakeLists.txt:54` calls `corrosion_import_crate()`
   **unconditionally**, so every consumer that `add_subdirectory`s nano_ros
   builds `--manifest-path <repo>/packages/api/nros-cpp/Cargo.toml` — root
   workspace context.
2. A workspace that has Rust nodes ALSO gets the synthesised umbrella
   (`cmake/NanoRosRuntimeCrate.cmake`, phase-241 W11 "Option D"):
   `nros_ws_runtime`, its own workspace, carrying
   `nros-cpp = { path = "<repo>/packages/api/nros-cpp" }` as an out-of-workspace
   path dep.

Corrosion derives its target dir from `CMAKE_BINARY_DIR` and offers **no
override** — phase-344 measured exactly this — so both land in
`<build>/cargo/build`. Cargo computes a different `-C metadata` per workspace
root, `deps/` accumulates both, and the umbrella staticlib bundles a mix: some
member crates were compiled against one `nros_rmw_cffi`, some against the other.
Every `#[no_mangle]` export then appears twice.

`cargo metadata` on the umbrella reports **one** `nros-rmw-cffi` package, which
is why this does not look like a dependency-graph problem. It is not one — the
graph is fine; the *identity* is not.

Only **mixed** workspaces fail: the collision needs the umbrella (Rust nodes
present) AND the unconditional plain import. Pure-C/C++ and pure-Rust
configures have one root each.

## Why the plain archive is built at all here

In this workspace the plain `libnros_cpp.a` is on **no executable link line** —
it is produced only so a header rule can consume it
(`nros_cpp_config_generated.h` / `nros_config_generated.h`). The umbrella is
what the 25 real link lines use. `NanoRosRuntimeCrate.cmake:19` states the
intended split: "pure-C / pure-C++ workspaces keep `nros-cpp-headers` pointed at
the plain `nros_cpp-static`" — i.e. the plain import was meant as the *fallback*
for configures with no Rust node, but it is imported unconditionally.

## Fixes considered

**A — key the target dir by workspace ROOT (principled).** phase-340 established
that workspace root is an incompatibility axis; the corollary is that a target
dir must never be shared across two of them. Corrosion has no override, so this
needs the plain import to land in its own CMake binary dir (a nested
`add_subdirectory` with its own `cargo/`) or an equivalent isolation. Costs a
second build tree; fixes the class rather than this instance.

**B — do not build the plain staticlib when the umbrella exists (cheapest).**
Take the generated headers from the umbrella's own `nros-cpp` build and skip the
plain `corrosion_import_crate` in that configure. This matches what the code
comment already says the design is; the bug is the missing condition. Needs the
header rule re-pointed, and needs care that a consumer adding `nano_ros` for
headers alone still works.

**C — stop bundling `nros-cpp` in the umbrella and link the plain archive.**
Rejected: Option D exists precisely to get rid of
`--allow-multiple-definition` and to split the stateful REGISTRY, so this
reintroduces what it was built to remove.

Recommendation: **B** to unblock, with **A** recorded as the class fix, because
B leaves two roots capable of sharing a dir the moment another consumer imports
a root-workspace crate.

## Connection to the phase-340 identity budget

This is very likely the same mechanism behind the disputed identity reading in
this exact tree. `check-artifact-identity-budget` reads
`examples/workspaces/mixed/build-workspace-fixtures`, and on a long-lived tree
here `nros` measures **12** identities = 2 workspace roots × 2 R3 halves × 3
feature sets, against a ceiling of 6 set by another session on a tree where the
duplication had not occurred. Fixing this should collapse the count — and settle
that disagreement — rather than the ceiling needing to move.

## Not verified

Whether this reproduces in the distrobox lane and in CI. It reproduces here on
every `lane=native` attempt, and it is a property of the CMake graph rather than
of the host toolchain, so a host-specific cause is unlikely — but that is
reasoning, not a measurement.
