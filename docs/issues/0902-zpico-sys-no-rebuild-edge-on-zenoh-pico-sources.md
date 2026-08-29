---
id: 902
title: "Editing zenoh-pico rebuilds nothing — `zpico-sys` watches 7 hand-listed
  files out of the whole library, so a patch is silently not compiled"
status: open
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
