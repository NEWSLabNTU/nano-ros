---
id: 1005
title: "A zenoh constant that lives in `nros-zpico-build` is invisible to the
  fixture staleness probe, so a fixture baked before a fix reports FRESH"
status: open
type: bug
area: testing, build
severity: high
found: 2026-09-03
related: [issue-0877, issue-0906, issue-0196, issue-0911, phase-414]
---

## What happens

The zenoh fixture staleness arm resolves its inputs from the
`cargo:rerun-if-changed` lines that `zpico-sys`'s build script RECORDED
(`packages/testing/nros-tests/src/fixtures/binaries/mod.rs:1240-1250`). Dumping
the recorded set for the FreeRTOS C talker gives 41 entries, and **none of them
is under `packages/rmw/zenoh/nros-zpico-build/`**.

That crate is a build-script DEPENDENCY. Cargo tracks it correctly through its
own unit graph — change it and cargo rebuilds — but it never appears as a
`rerun-if-changed` PATH, and the path list is what the probe reads. So a
constant that lives there is outside everything the probe examines.

## Measured

`Z_TRANSPORT_LEASE_MS = 60_000` at
`packages/rmw/zenoh/nros-zpico-build/src/lib.rs:289` (issue 0906's fix,
2026-08-30). Every built FreeRTOS fixture in the tree still bakes the old value:

    examples/qemu-arm-freertos/{c,cpp}/*/build-zenoh/cargo/.../zpico-sys-*/out/
        zenoh-config/zenoh_generic_config.h
        #define Z_TRANSPORT_LEASE 10000      <- all 20 of them

    examples/qemu-arm-freertos/c/talker/build-zenoh/c_talker   mtime 2026-08-21

Source says 60000. Binaries say 10000. The probe reports FRESH.

## Why it matters more than a stale binary

Issue 0906 measured what the old value costs on exactly these images: **19 heard
of 77** before, **77 of 77** after, because a 10 s lease against a router
keep-alive on a 30 s cadence expires deterministically. So the probe is reporting
FRESH on binaries that carry a known, measured, delivery-breaking defect.

Found while rediagnosing issue 0877 (phase-414 W1), whose "0 messages received"
is very likely this: the report is dated one day before 0906's fix, and the
fixtures on disk have not moved since.

**This is the shape CLAUDE.md already names**: "Build-side stale probes must
watch the same inputs as test-side gates — a probe that misses `generated/**`
lets a museum binary pass every sweep" (issue 0196). Same rule, a different
input class: not a generated tree, but a build-script dependency crate.

## Why the usual reasoning does not save it

* It is not a `--target-dir` or profile confusion (issue 0488's class): the
  binaries are in the right place, they are simply old.
* It is not the exemption machinery (issue 0442/0445): nothing is exempted here.
  The input was never a candidate.
* A STALE verdict is absorbing and loud; a FRESH verdict is silent. This is the
  silent direction, which is the worse one.

## Direction

Not settled, and worth choosing rather than patching the one constant:

1. **Walk the build-script dependency closure.** `zpico-sys`'s build script
   depends on `nros-zpico-build`; the probe could resolve that from cargo
   metadata rather than from recorded paths. Closest to correct, and the same
   answer as `packages/cli/cli-source-dirs.txt` reached for the CLI's own
   freshness closure (issue 0627) — that one is GENERATED for exactly this
   reason, after a textual walk was found wrong in both directions.
2. **Have `nros-zpico-build` emit its own sources as `rerun-if-changed`** from
   the consuming build script, so the recorded list becomes complete. Cheapest;
   relies on every future build-script dependency remembering to do it, which is
   the property that failed here.
3. **Hash the generated `zenoh_generic_config.h` into the probe's input set.**
   Watches the OUTPUT rather than the inputs, so it cannot miss an input class —
   but it only covers what that header carries.

Whichever lands, the acceptance is the case above: change
`Z_TRANSPORT_LEASE_MS`, do not rebuild, and require the probe to report STALE.

## Not covered

Whether other build-script dependency crates feed other fixture families the
same way. `nros-zpico-build` is the one measured; the class is "a build-script
dependency crate", and nothing has swept for siblings.
