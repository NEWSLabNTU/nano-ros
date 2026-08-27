---
id: 834
title: "The per-build `nros_cpp_config_generated.h` mirror can reach a state no
  re-run repairs — only wiping the west build dir recovers it"
status: open
type: bug
area: cmake
related: [issue-0088, issue-0114, issue-0122, issue-0123, issue-0245, issue-0268, issue-0196]
---

## Problem

The sizes-header mirror family (0088 → 0114 → 0122 → 0123 → 0245 → 0268) has
always been a RACE or a MISSING EDGE: the mirror ran too late, or not at all,
and the fix was ordering. This is a different failure — the mirror reaches a
state that is **absorbing**. Re-running the build does not repair it; the only
recovery found was `rm -rf` on the west build directory.

Hit during a `lane=all` fixture build (phase-383 W9.c). Two of ~40 zephyr leaves
failed, both XRCE C:

```
zephyr-fixture-25-build-c-talker-xrce
zephyr-fixture-27-build-c-service-server-xrce

packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h:59:2:
  error: #error "nros_cpp_config_generated.h must be supplied per-build by the build system"
```

The source-tree STUB won because the per-build header was not in the mirror
directory. The stub's `#error` is working exactly as designed — it is the only
reason this surfaced as a build failure rather than as wrong `*_OPAQUE_U64S`
values, which is the 0245/0268 outcome.

## What was observed, in order

A survey found exactly two build dirs in a broken shape — mirror directory
holding the `.stamp` and **not** the header:

```sh
for d in zephyr-workspace/build-*/nros-rust/nros-{cpp,c}-generated/nros; do
  h=$(ls $d/*.h 2>/dev/null | wc -l); s=$(ls $d/*.stamp 2>/dev/null | wc -l)
  [ "$s" -gt 0 ] && [ "$h" -eq 0 ] && echo "ORPHAN STAMP: $d"
done
# ORPHAN STAMP: zephyr-workspace/build-c-service-server-xrce/.../nros-cpp-generated/nros
# ORPHAN STAMP: zephyr-workspace/build-c-talker-xrce/.../nros-cpp-generated/nros
```

A sibling zenoh build had both files, same timestamp.

1. **Deleting the stamp did not help.** The leg failed again, same error.
2. **Building the header target directly did not help.** ninja knows the edge —
   `ninja -t query` shows the header as a `CUSTOM_COMMAND` output — and the
   command RAN:

   ```
   [1/2] Building nros-c via Cargo     ... Finished in 0.63s
   [2/2] Building nros-cpp via Cargo   ... Finished in 0.92s
   ```

   and produced nothing. The mirror directory stayed empty.
3. **The copy source was missing too** — no `nros_cpp_config_generated.h`
   anywhere under that build dir, so this is not a copy that failed; the
   byproduct was never re-emitted.
4. **`rm -rf` on the two build dirs fixed it.** The leg then went green.

## Inferred mechanism

The header is a build-script BYPRODUCT written into the cargo `OUT_DIR` and
mirrored out by a POST_BUILD copy. Once cargo considers the crate up to date it
prints `Finished` without re-running the build script, so the byproduct is not
re-emitted and the copy has nothing to copy — while ninja, seeing its
`CUSTOM_COMMAND` complete successfully, records the output as built. Any state
that removes the destination without invalidating cargo's fingerprint is
therefore permanent.

Stated as inference: what is CERTAIN is (1)–(4) above. What made these two
particular dirs lose their header, while their zenoh siblings kept it, is not
established. Both are XRCE, which is suggestive and not conclusive on n=2.

## Why it matters more than the count suggests

* It cost a full `lane=all` fixture build. The zephyr leg is not `-k`: one leaf
  stopped the sweep, and the sweep is the multi-hour prerequisite for
  `just ci-matrix`.
* Every documented remedy for this family — re-run, clear the stamp, rebuild the
  target — is ineffective here, so the natural debugging path is the wrong one
  and ends at "it just fails".
* The existing guard is the STUB's `#error`, which catches C++ TUs. The nros-C
  mirror (`nros_config_generated.h`) has the same shape and the same
  `.stamp`-beside-header layout; a missing header there surfaces as
  `SESSION_OPAQUE_U64S undeclared`, which issue 0088 records as having been
  latent for a whole phase.

## Directions

1. **Make the mirror a real edge with the byproduct as a declared input**, so
   ninja rebuilds the copy when the destination is missing rather than trusting
   a successful custom command. `nros_c_config_header` already exists as a real
   custom target for exactly this reason (issue 0088) — the cpp side leans on
   `cargo-build_nros_cpp`'s POST_BUILD, and the comments in
   `cmake/NanoRosRuntimeCrate.cmake` say so.
2. **Or make the stamp load-bearing rather than decorative** — if the stamp is
   meant to record that the mirror is current, it should be compared against the
   destination's existence, and a stamp with no header should force a re-copy.
   Today it is neither checked nor sufficient.
3. **Add the orphan-stamp survey above as a gate.** It is three lines, it runs
   in milliseconds over the whole zephyr workspace, and it names the exact
   directories — which is more than the `#error` does, and it catches the nros-C
   side too, where the failure is not self-describing.

## Sweep

```sh
grep -rn 'nros_cpp_config_generated\|nros_config_generated' cmake/ | grep -v '^cmake/.*#'
grep -rn 'POST_BUILD' cmake/NanoRosRuntimeCrate.cmake packages/api/nros-cpp/CMakeLists.txt
```
