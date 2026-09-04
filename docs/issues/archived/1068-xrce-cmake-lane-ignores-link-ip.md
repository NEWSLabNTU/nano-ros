---
id: 1068
title: "`NROS_LINK_IP=0` drops the UDP transports in the Rust lane and not in the CMake one, so a serial-only XRCE node cannot be built from C/C++"
status: resolved
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

## Fixed — the list is derived, not mirrored

`packages/rmw/xrce/xrce-sources.txt` is now the ONE list, and neither lane holds
a source path of its own:

* **Format.** Line-oriented, `#` comments, whitespace-separated columns — the
  shape `config/rust-targets.txt` and `packages/cli/cli-source-dirs.txt`
  already use here, and the only shape both consumers can read. CMake has no
  TOML parser (`file(STRINGS)` is what it has) and the Rust side must not gain
  a dependency (`cargo` is `--locked`-shimmed). Two record types:
  `group <name> <condition>` and `src <group> <tree> <path>`, with `<tree>` one
  of `uxr` / `ucdr` / `backend`.

* **The conditionals are part of the fact.** A manifest that listed files
  without saying which are conditional would have moved the drift one file
  over, so the CONDITION lives on the group, in the manifest. A lane supplies
  only a boolean per condition TOKEN (`always`, `posix`, `posix_ip`) and never
  decides which group a token covers. `build.rs` answers them in a
  `NROS-XRCE-CONDITIONS` block (`is_posix`, `is_posix && ip`); the CMakeLists
  answers them in the matching block (`set(_xrce_cond_*)`).

* **The behaviour fix.** The CMake lane now resolves `NROS_LINK_IP` from
  `-DNROS_LINK_IP=…` or the environment, and gates BOTH halves on it: the
  `uxr_ip_udp` source group and the `UCLIENT_PROFILE_UDP` / `_TCP` defines in
  the generated `<uxr/client/config.h>`. Measured: 41 TUs with IP on (byte-for-
  byte the pre-change set, and the same 41 the Rust lane compiles), 39 with
  `NROS_LINK_IP=0`, both linking and passing the two ctest cases.

* **micro-CDR rides the same mechanism** — its five files are the `ucdr` group.
  (Its VERSION string is issue 1069 and is untouched here.)

### `is_posix` was NOT the same defect

The Rust lane gates `util/time.c` and `transport_posix_{udp,serial}.c` on
`is_posix`; the CMake lane compiled them unconditionally. That looks like the
same shape and is not, because the CMake project is POSIX-only BY CONSTRUCTION:
it `FATAL_ERROR`s without an `nros_platform_posix` target, defines
`_POSIX_C_SOURCE=200809L` unconditionally, and configures
`UCLIENT_PLATFORM_POSIX TRUE` with no fanout. It has no notion of a non-POSIX
target for the two lanes to disagree about, so both select the same files. The
condition is spelled out in that lane anyway (`set(_xrce_cond_posix TRUE)` with
the reason) so the notion exists by name and a future cross-compiling CMake lane
has one line to change.

### Gate

`scripts/check-xrce-source-manifest.py` — recipe `just check xrce-source-manifest`
(fast line; no build artifacts, no SDK). It fails when the manifest is
inconsistent, when it lists a file that does not exist, when **either lane names
a `.c` of its own** (the check that makes the mirror unrecreatable), when the two
lanes do not answer exactly the manifest's condition-token set, or when a backend
`.c` is compiled by neither lane. Its self-test runs on the normal path.

Demonstrated red six ways and restored: a source added by hand to the CMake
target; one added to `build.rs`; a manifest token neither lane answers; a
manifest token only `build.rs` answers (the 1068 shape exactly); a manifest entry
naming a missing file; a backend source dropped from the manifest.
