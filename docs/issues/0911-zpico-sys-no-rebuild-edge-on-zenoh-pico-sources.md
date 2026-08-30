---
id: 911
title: "Editing zenoh-pico rebuilds nothing — `zpico-sys` watches 7 hand-listed
  files out of the whole library, so a patch is silently not compiled"
status: resolved
type: bug
area: build, rmw
related: [issue-0475, issue-0820, issue-0899]
---

## Symptom

Patch a zenoh-pico source, rebuild, run: the change is not there. No error, no
warning — the old code runs and the test result is about a binary that does not
contain the edit.

Measured while working issue 0899: an edit to
`zenoh-pico/src/protocol/iobuf.c`, a full fixture rebuild (`rc=0`), and a
reproduction that behaved exactly as before, because `iobuf.c` had never been
recompiled.

## Cause

`nros-zpico-build/src/runner.rs` names its inputs one at a time:

    println!("cargo:rerun-if-changed=zenoh-pico/src/system/unix/network.c");
    println!("cargo:rerun-if-changed=zenoh-pico/include/zenoh-pico/system/platform/unix.h");
    println!("cargo:rerun-if-changed=zenoh-pico/src/system/freertos/system.c");
    println!("cargo:rerun-if-changed=zenoh-pico/src/system/freertos/lwip/network.c");
    println!("cargo:rerun-if-changed=zenoh-pico/src/net/primitives.c");
    println!("cargo:rerun-if-changed=c/zenoh-pico-version.h.in");
    println!("cargo:rerun-if-changed=zenoh-pico/version.txt");

Seven files. zenoh-pico has hundreds, and the build COMPILES them all — the
protocol, transport, collections and codec sources that carry almost every bug
worth patching are absent from the list. Cargo therefore considers the build
script up to date and never re-runs it.

This is issue 0820's finding in a second crate, and its words apply unchanged:
"Hand-listing the closure here would be a maintained approximation of a graph
[the compiler] already computes exactly."

## Why it is worse than a slow build

A missing `rerun-if-changed` does not fail. It produces a museum binary that
tests green, so the conclusion drawn from the run is about code that is not in
it. During 0899 this cost an entire measure-fix-measure cycle and nearly
produced a "this fix does not work" verdict about a fix that had never been
compiled.

## Not the only edge missing on this path

Even after `zpico-sys` recompiles, the C example binary is NOT relinked — the
build log shows `Compiling zpico-sys` and never mentions `c_talker`. That is
issue 0475's class (a lib reached through a whole-archive link flag gets no
rebuild edge), and it means a zenoh-pico patch needs BOTH edges before an
example image contains it. Today the only reliable way to get an edit into an
image is to touch the leaf's own `main.c` as well.

## Direction

Watch the directory, not a list. `cargo:rerun-if-changed` accepts a directory
and cargo walks it, so `zenoh-pico/src` and `zenoh-pico/include` would cover the
compiled set with two lines and no maintenance. The cost is that any touch under
those trees re-runs the build script; for a vendored library that changes only
when someone patches it, that is the correct trade.

## Acceptance

* Editing any compiled zenoh-pico source causes it to be recompiled.
* An edit reaches the C/C++ example images without hand-touching their sources,
  or the remaining gap is named where a patcher will read it.

## Renumbered from 0902

This shared id 902 with "action goal completion is variable" -- two unrelated
open issues under one number, which is exactly the confusion this ledger exists
to prevent. Renumbered to 911; the action issue keeps 902.

## Resolved

`nros-zpico-build` now watches `zenoh-pico/src` and `zenoh-pico/include` as
directories instead of naming five files, so cargo re-runs the build script for
any edit under the compiled set.

Verified against the acceptance criteria:

* a no-op rebuild recompiles nothing
* an edit to `src/protocol/iobuf.c` -- absent from the old list, and the exact
  file whose silent non-rebuild cost a cycle in issue 0899 -- recompiles the
  crate, and the new symbol is present in the resulting `iobuf.o`

The second acceptance bullet (the C example relink, issue 0475's class) is NOT
closed by this change and is left to that issue. On the Zephyr path it does not
arise: zenoh-pico is compiled by the Zephyr CMake through a glob, and ninja
tracks those objects directly.

Fixing this also surfaced that the cargo path did not build against zenoh-pico
1.10 at all -- the Zephyr path masked it by generating its own defines. Both
causes are recorded in the fix commit: the `@TOKEN@` tunables were never emitted
without upstream's CMake, and two platform manifests named source files that
1.10 deleted.
