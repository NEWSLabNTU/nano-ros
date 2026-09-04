---
id: 1068
title: "`NROS_LINK_IP=0` drops the UDP transports in the Rust lane and not in the CMake one, so a serial-only XRCE node cannot be built from C/C++"
status: open
type: bug
area: rmw, build
severity: medium
found: 2026-09-05
related: [1069, phase-420, phase-321]
---

# One vendored tree, two compilers, one honouring the knob

Micro-XRCE-DDS is compiled twice — once by `nros-rmw-xrce-cffi/build.rs`
(lines 160–245) and once by `nros-rmw-xrce/CMakeLists.txt` (143–193), from a
hand-copied source list. The two have drifted.

`build.rs` honours `NROS_LINK_IP=0` (phase-204.7's per-build override, the one a
serial-only node uses to shed the IP link) and drops
`udp_transport{,_posix}.c`. The CMakeLists compiles them unconditionally.

So the knob works from Rust and silently does nothing from C or C++: **a
serial-only XRCE node cannot be built through the CMake lane**, and the failure
is not a build error — it is a larger image than the user asked for, with an IP
transport they deliberately removed.

## Why the mirror is unguarded

The lockstep between the two lists is asserted by a comment, and that comment
names `packages/rmw/xrce/xrce-sys/build.rs` — a file phase-321 W1.d **deleted**
when it removed the `xrce-sys` crate ("zero dependents"). So the only thing
holding the two lists together points at nothing.

Found by the phase-420 W9 survey, which was measuring how each vendored tree is
obtained and found it is obtained once and compiled twice.

## Fix

The source list should be derived once and consumed by both lanes, rather than
mirrored. That is the same remedy class as `check-ffi-struct-mirrors` (a
hand-mirrored thing drifts on append) and the sizes-header family — and until it
exists, a gate comparing the two lists would at least make a divergence loud.

Do not fix it by editing the CMake list to match today's `build.rs`: that
restores the mirror and leaves the next append to drift again.
