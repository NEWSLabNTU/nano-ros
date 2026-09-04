---
id: 1073
title: "`transport_zephyr_udp.c` is compiled by neither lane — superseded by `transport_nros_udp.c` in phase 129.C.1 and never deleted"
status: open
type: bug
area: rmw
severity: low
found: 2026-09-05
related: [1068, 1069]
---

# A translation unit nothing builds

`packages/rmw/xrce/nros-rmw-xrce/src/transport_zephyr_udp.c` is compiled by
neither the cargo lane nor the CMake one. Phase 129.C.1 superseded it with
`transport_nros_udp.c`, which reaches the platform's UDP through
`nros_platform_udp_*` and therefore works on every target; `build.rs` still
carries the `feat_zephyr = false` that switched the old one off, and `git grep`
finds no other reference to it.

Found while building issue 1068's source manifest: the new
`check-xrce-source-manifest` asserts that every backend `.c` is compiled by at
least one lane, and this is the file that made that check fail on its first run.

## Why it is filed rather than deleted

Deleting it was out of scope for 1068, which was a refactor plus one named
behaviour fix. It sits on the gate's documented `NOT_COMPILED` list, so
"nothing builds it" is now a **recorded fact rather than a silence** — which is
the state that matters. Removing the file is the follow-up.

## Before deleting, check

Whether anything outside this repository names `xrce_zephyr_udp_init`. The
symbol was a public entry point for one release, and phase 129.C.1's note says
the posix pair was "kept alongside `transport_nros_udp` for one cycle so callers
that still resolve `xrce_posix_udp_init` keep working" — the same courtesy may
have been intended here. If nothing does, delete the file and its
`NOT_COMPILED` row together; the gate refuses a row naming a file that no longer
exists, so the cleanup cannot be half-done.
