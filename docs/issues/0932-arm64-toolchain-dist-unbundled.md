---
id: 932
title: "The linux-arm64 arm-none-eabi-gcc dist gets neither the ncurses bundle
  nor the gdb Python, and its gdb fails EARLIER than x86_64's did"
status: open
type: bug
area: tooling
related: [0926, 0928, 0929]
---

## What is missing

`arm-none-eabi-gcc` 13.2-nros3 ships two fixes on **linux-x86_64** and neither
on **linux-arm64**, whose tarball is byte-identical to -nros2 (same sha256 —
that identity is how the release proved it touched only one host):

* **ncurses 5 bundling** (issue 0928). Skipped because the arm64 leg then failed
  on the NEXT library.
* **the gdb Python stdlib** (issue 0929). Skipped because the arm64 gdb needs
  more than a stdlib.

## Why arm64 is a different problem, not the same one deferred

The two hosts fail at different STAGES, and the arm64 one fails earlier:

| | linux-x86_64 | linux-arm64 |
| --- | --- | --- |
| Python | STATIC in gdb; no `libpython` in `NEEDED` | DYNAMIC — links `libpython3.8.so.1.0` |
| failure | interpreter init aborts (stdlib absent) | the LOADER fails; gdb never starts |
| fix | ship the stdlib, point `PYTHONHOME` | needs the `.so` too, and 22.04 packages no python3.8 |

Measured during the -nros2 build: `bundle: cannot resolve libpython3.8.so.1.0`.
The bundler refused, correctly — it fails rather than shipping a dist bundled
everywhere except the one library it could not resolve.

Also note the ncurses spelling differs by architecture: x86_64 links
`libncursesw.so.5`, arm64 links `libncurses.so.5`. A package list derived from
one host is wrong for the other by construction, which already cost one build.

## What it would take

The blocker is obtaining `libpython3.8.so.1.0` for arm64 on a 22.04 runner:

1. **deadsnakes PPA** — cheapest, but adds a third-party apt source to a release
   build, which is a supply-chain decision rather than a packaging one.
2. **Build CPython 3.8 in the job** — self-contained and slow, and it makes the
   toolchain repackage a source build, which it deliberately is not today.
3. **Take the `.so` from python.org's build** — no compiler needed, but their
   binaries are not distributed for Linux, so this may not exist.
4. **Accept and declare** — leave arm64 as-is and let `smoke` report `[BROKEN]`
   there, which is honest and costs nothing. `system = ["libcrypt1"]` already
   holds for both hosts.

Direction 4 is the current de-facto state and is not wrong; this issue exists so
it is a CHOICE rather than an oversight.

## Verifying any fix

Nothing here can be checked on an x86_64 developer host. The release job's own
`gdb-python: OK` assertion runs per host, so an arm64 fix proves itself in CI —
and `nros setup --tool arm-none-eabi-gcc --check` reports `[BROKEN]` on an arm64
host until it lands, via the `smoke` probe (phase-404 / issue 0929).
