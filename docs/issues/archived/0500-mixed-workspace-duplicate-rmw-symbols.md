---
id: 500
title: "A stale SDK Corrosion shadows the installed pin: glob-ordered store prefixes made examples/workspaces/mixed unlinkable"
status: resolved
type: bug
severity: high
area: build, orchestration
related: [issue-0493, issue-0491, issue-0488, phase-340]
resolved_in: "issue-0500 (version-ordered Corrosion store prefixes)"
---

## Symptom

`examples/workspaces/mixed` fails to link from a CLEAN build dir. Seven
duplicate-symbol errors, every one of them an RMW registration symbol:

```
/usr/bin/ld: libnros_ws_runtime.a(nros_rmw_zenoh-34d9a139….rcgu.o): in function
  `<nros_rmw_cffi::rust_adapter::RustBackendAdapter<…ZenohRmw>>::register_named':
  packages/rmw/cffi/src/rust_adapter.rs:357: multiple definition of `nros_rmw_zenoh_register';
  libnros_ws_runtime.a(nros_rmw_zenoh-852ab7d5….rcgu.o): first defined here
… also REGISTRY, nros_rmw_cffi_lookup, nros_rmw_cffi_register,
  nros_rmw_cffi_register_named, nros_rmw_cffi_registered_names,
  nros_rmw_cffi_set_custom_transport
```

It kills the whole native fixture lane at `ws-group-0`, so **`just
build-test-fixtures lane=native` and `lane=all` cannot complete** — every
workspace scheduled after `mixed` never builds, and the tests that need those
artifacts then fail as "not prebuilt". `lane=tier2` is unaffected only because
its narrowed row set excludes the failing row.

Reproduce (~15 min, no other lane needed):

```
rm -rf examples/workspaces/mixed/build-workspace-fixtures
bash scripts/build/workspace-fixtures-build.sh linux mixed
```

## Not stale residue, and not new

Measured, because the first reading was wrong twice:

* **Not residue.** It reproduces from a wiped `build-workspace-fixtures`. An
  earlier "fix" by wiping only appeared to work because the retry built ONE
  fixture id; the duplicate needs the whole workspace (both the C and the C++
  entry) to be in the same build.
* **Not new.** Identical signature — exit 2, the same 7 errors — at
  `91a76b133`, the parent of the commits that were suspected of causing it, and
  still identical after pulling 24 commits including #493's corrosion
  unification and the v0.5.1 → v0.6.1 pin bump.

## Measured cause: two path SPELLINGS of one crate

Both rlibs live in the same target dir:

```
…/mixed/build-workspace-fixtures/cargo/build/x86_64-unknown-linux-gnu/nros-relwithdebinfo/deps/
  libnros_rmw_zenoh-34d9a1395e2547d5.rlib
  libnros_rmw_zenoh-852ab7d5d02b3ee5.rlib
```

Their fingerprints agree on everything that would normally explain a split —
`features: ["platform-posix"]` on both, same profile, same target triple. Three
fields differ: `deps`, `local`, and **`path`**. The dep-info files say why:

| identity | source path as passed to rustc |
| --- | --- |
| `34d9a139…` | `/home/aeon/repos/nano-ros/packages/rmw/zenoh/nros-rmw-zenoh/src/lib.rs` |
| `852ab7d5…` | `packages/rmw/zenoh/nros-rmw-zenoh/src/lib.rs` |

Absolute vs relative — the same directory, spelled two ways, so cargo
fingerprints them as two units and emits two `-C metadata` identities. The
generated umbrella manifest names its deps absolutely:

```toml
# …/mixed/build-workspace-fixtures/nros_ws_runtime/Cargo.toml
nros-cpp = { path = "/home/aeon/repos/nano-ros/packages/api/nros-cpp", … }
rust_heartbeat_pkg = { path = "/home/aeon/repos/nano-ros/examples/workspaces/mixed/src/rust_heartbeat_pkg" }
```

while the node package it pulls in names its own dep relatively:

```toml
# examples/workspaces/mixed/src/rust_heartbeat_pkg/Cargo.toml
nros = { path = "../../../../../packages/api/nros", … }
```

`mixed` is the only workspace that puts a Rust node package and the C++ API
under one umbrella staticlib, which is why it is the only one that fails.

`852ab7d5…` is worth noting on its own: it is byte-identical across every run,
across a full wipe, and across commits that change the metadata of everything
that sees them. Whatever produces that unit is not seeing the same inputs as the
rest of the build.

## Why the archive ends up with both

`libnros_ws_runtime.a` is the corrosion-built staticlib for `nros_ws_runtime`,
and the ninja log shows that package built TWICE in one workspace build (two
`cargo rustc --package nros_ws_runtime` invocations into the same
`--target-dir`). Two builders sharing one target dir, disagreeing on how to
spell a path, is the phase-340 artifact-identity theme (#493 "one corrosion
resolution for every builder", #491 "fingerprint path build inputs by CONTENT,
never by env spelling"). Whether the fix belongs in the manifest generator (emit
ONE spelling), in the umbrella's dep set, or in the shared-target-dir policy is
not yet established — this issue records the measurement, not the design.

## Impact

* `lane=native` / `lane=all` fixture builds cannot complete, so any tier that
  needs the full existence set (tier 2's run included) cannot be trusted.
* Every `mixed` workspace runtime cell is unbuildable.

## Root cause: the store is enumerated in GLOB order, so a stale entry shadows the pin

Not the workspace, not the manifest generator — the build was using **Corrosion
0.5.1** the whole time, on a host where 0.6.1 was installed. Corrosion's version
decides the cargo target-dir topology (this is #0493's finding): `< 0.6.0` names
the dir with a constant, so two workspace roots configured into one binary dir
share a `deps/`, and their `#[no_mangle]` exports collide. The two path
spellings measured above are that shared `deps/` seen from two builders — a
symptom, not the cause.

What hid it is worth stating plainly: **the provisioning step appeared to
work.**

```
$ just workspace install-corrosion
[corrosion] installed v0.6.1 at /home/aeon/.nros/sdk/corrosion
$ nros setup --tool corrosion
-- Installing: /home/aeon/.nros/sdk/corrosion/0.6.1-nros1/lib/cmake/Corrosion/CorrosionConfig.cmake

# and the very next configure, from a DELETED build dir:
-- nano-ros: Corrosion 0.5.0 via SDK store [hashless shared cargo/build — issue 0493 link risk]
   — /home/aeon/.nros/sdk/corrosion/0.5.1-nros1/lib/cmake/Corrosion
```

`_nros_corrosion_prefixes` globbed `$NROS_HOME/sdk/corrosion/*`, kept every dir
with a resolvable `CorrosionConfig.cmake`, and `find_package` takes the FIRST
that resolves. The store accumulates, glob order is lexicographic, so
`0.5.1-nros1` outranked the `0.6.1-nros1` a provisioning run had just written.
Renaming the stale entry to a dotted name did not help — the glob matched it
anyway; only moving it out of the store did.

Only #0493's reporting line made this visible at all. Without it the second
install would have been recorded as "rebuilt on v0.6.1, still broken", and the
investigation would have gone back to the manifest generator.

## Fix

* `_nros_corrosion_prefixes` (cmake) sorts the versioned store dirs
  `COMPARE NATURAL ORDER DESCENDING`; `nros_cmake_corrosion_prefixes` (shell)
  uses `sort -Vr`. Newest version first, flat prefix last, in both. NATURAL /
  `-V` rather than lexicographic so `0.10.x` outranks `0.9.x`.
* `check-cmake-corrosion-prefix` now asserts both spellings, so the ordering
  cannot regress in one derivation and not its sibling — which is the shape
  #0493 was filed for. Mutation-tested: breaking either half fails the gate.

## Verified

`examples/workspaces/mixed`, wiped build dir, **both** versions present in the
store so the ordering is what decides:

```
-- nano-ros: Corrosion 0.6.1 via SDK store [hashed per-workspace cargo dirs]
exit=0, 0 duplicate-symbol errors
```

Before the fix, the identical command resolved 0.5.1 and produced 7.
