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

## Investigated 2026-08-29 — one mechanism reproduced, one still unrooted

### The inferred mechanism was wrong in its details

The report inferred: "cargo considers the crate up to date, prints `Finished`
without re-running the build script, so the byproduct is not re-emitted".

Half right, and the wrong half matters. The build script **does** run, and it
declines to write:

```rust
// packages/tooling/nros-build-helpers/src/cpp.rs:230
if exact_executor == 0 {
    // `cargo check --no-default-features` / `cargo doc` path — probe yielded
    // nothing. Skip writing the per-build header.
    return;
}
```

Reproduced in seconds on the host copy, on today's tree:

```
$ rm target/nros-cpp-generated/nros/nros_cpp_config_generated.h
$ cargo build -p nros-cpp
warning: nros-cpp: EXECUTOR_SIZE probe returned 0 — likely a
         `cargo check --no-default-features` run
    Finished `dev` profile in 0.10s
$ ls target/nros-cpp-generated/nros/*.h    # still absent
```

The script ran — its warnings printed — and wrote nothing. Touching `build.rs`
does not change it, because the script is not being skipped; it is returning.
A build WITH a backend feature restores the header immediately.

So the absorbing property is not "cargo won't re-run the script". It is: **the
only writer may legitimately decline, and nothing notices the result.** The
decline is correct on its own terms — the comment is right that no RMW backend
means no executor sizes to ship, and the stub's `#error` is the intended
outcome for that configuration. What is wrong is that the same decline, over a
target dir where the header is missing for some OTHER reason, reports success
and leaves consumers reading the stub.

### Also established

* `write_header_to_target_dir` writes header then stamp, unconditionally, in
  one function — so the pair the survey found (stamp, no header) is a state the
  writers **cannot produce**.
* `write_header_if_absent_or_verify` and `write_header_to_target_dir` both
  self-heal an absent header — but only when reached.
* `write_atomic` is genuinely atomic (temp + rename, panics on failure), so a
  half-written header is not the explanation.
* The `add_custom_command` mirror **does** declare its destinations as `OUTPUT`
  and the mirror script exits 1 on a missing source, naming issue 0805. Note
  that script landed at 21:04 on 2026-08-27 — **7.5 hours after this issue was
  filed at 13:38** — so the report's step (2) was observed against a mechanism
  that has since changed.

### Still unrooted

**How the two XRCE build dirs lost their header while keeping the stamp.** The
recovery was `rm -rf`, which destroyed the evidence. Nothing in the tree deletes
a `.h` without its `.stamp`; no such code path was found.

That is why the fix below is a GATE and not a repair: the state is detectable in
milliseconds and self-describing, whatever produced it, whereas a repair for an
unrooted cause would be a guess.

## Fixed — `check-orphan-generated-stamp` (fast lane)

The survey from this report, as a gate. Walks both generated trees
(`nros-c-generated` and `nros-cpp-generated` — the C side matters most, since
its symptom is `SESSION_OPAQUE_U64S undeclared` rather than a self-describing
`#error`, and issue 0088 records that being latent for a whole phase), names the
offending directory, and prints the only recovery that works.

**0.05 s, and no walk.** The first two versions walked the tree — 8.2 s naive,
2.0 s pruned, 221 000 directories to find 650 candidates — and
`check-no-tracked-file-find` (issue 0844) rejected that, correctly: its own
message says to scope an artifact scan to a build dir rather than to `examples/`
or `packages/`. The locations now come from the git INDEX (`git ls-files
'*Cargo.toml'` → the leaf dirs whose `target*/` siblings are the cargo target
dirs) plus the roots no manifest points at (`target/`, `build/`,
`zephyr-workspace/build-*/nros-rust`). Only those are stat-ed.

Mutation-checked against the real tree: removing the live header makes it fail
and name the exact path; restoring it makes it pass. Its own negative control
runs on every invocation and covers three cases — a healthy pair, the 0834
state, and a header with no stamp, which is NOT this defect and must not trip
it.

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
