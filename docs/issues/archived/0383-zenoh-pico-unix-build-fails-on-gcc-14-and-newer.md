---
id: 383
title: "The book's first node cannot build on gcc >= 14: vendored zenoh-pico's unix/network.c calls `_z_connect_serial` with no declaration"
status: resolved
type: bug
area: rmw
related: [issue-0373, issue-0135, rfc-0014]
resolved_in: "zenoh-pico 61ed48f + 07de44f + submodule pointer bumps"
---

# zenoh-pico unix build fails on gcc >= 14 (implicit declaration)

## Symptom

The book's Rust first node — `examples/native/rust/talker`, the page a new
user lands on — fails to build on a current-compiler host:

```
warning: zpico-sys@0.5.0: .../zenoh-pico/src/system/unix/network.c:951:32:
    error: implicit declaration of function '_z_connect_serial';
    did you mean '_z_close_serial'? [-Wimplicit-function-declaration]
error: failed to run custom build command for `zpico-sys v0.5.0`
```

Reproduced on Arch Linux, gcc 16.1.1, following installation.md +
first-node-rust.md verbatim on a clean host.

## Cause

`src/system/unix/network.c` calls `_z_connect_serial()` inside its
`Z_FEATURE_LINK_SERIAL == 1` block, but never includes
`zenoh-pico/system/common/serial.h`, where that function is declared. C99
removed implicit function declarations; **gcc <= 13 accepted the call with a
warning, gcc >= 14 (and clang >= 15) reject it**. `zpico-sys` builds with
`Z_FEATURE_LINK_SERIAL=1`, so every POSIX consumer compiles that block.

Not a nano-ros regression — the latent defect is upstream in the vendored
fork and only became fatal as compilers moved. That is why it appeared now
and on this host: CI images (Ubuntu 22.04 / 24.04) ship gcc 11 / 13, below the
threshold. Anyone on Arch, Fedora 40+, or a recent Debian testing hits it on
their first build.

## Fix

One-line include, guarded by the same feature macro as its only caller:

```c
#if Z_FEATURE_LINK_SERIAL == 1
#include "zenoh-pico/system/common/serial.h"
#endif
```

Landed as `61ed48f` on `jerry73204/zenoh-pico` `main` (fast-forward over
`0ef606e`, linear), with the superproject pointer bumped to it in the same
commit that archives this issue — fork first, pointer second, per the
vendored-fork rule.

Verified after the fix: `cargo build` in `examples/native/rust/talker`
succeeds, and the talker publishes end to end against the store `zenohd`
(`Publishing: 'Hello World: 1..9'`) on a host with no ROS 2.

## The class, not just the site

The compiler moved, not the code — so look for the siblings before calling
this closed:

1. Other `-Wimplicit-function-declaration` sites across the vendored C
   (zenoh-pico, mbedtls, micro-xrce-dds-client, FreeRTOS/lwIP, NetX). A
   `-Werror=implicit-function-declaration` build on one modern-compiler host
   enumerates them in one pass.
2. The same "CI compiler is older than a user's compiler" gap in general. Every
   image in use is Ubuntu; no lane builds on gcc >= 14. Issue 0373 filed the
   distro half of this (the install path was only exercised on ubuntu+bash);
   this is the toolchain half, and it bites C code rather than shell.

A cheap first step for (2): add one modern-compiler container to whatever lane
builds the native fixtures, or run `just probe bootstrap-arch` (added by 0373)
before releases — it would have caught exactly this.

## Second site, and a near-miss worth recording (2026-08-03)

The same class hit a second file: `src/link/endpoint.c` calls
`_z_custom_config_to_str` inside its `Z_FEATURE_LINK_CUSTOM == 1` branch while
including the config header for every OTHER link type (tcp, udp, bt, serial,
ivc, ws, tls, raweth) and not `custom.h`. With the feature on — which
`examples/native/rust/custom-transport-talker` turns on, that being its whole
point — the call is an implicit declaration returning `int`, returned as
`char *`:

    src/link/endpoint.c:547:16: error: returning 'int' from a function with
    return type 'char *' makes pointer from integer without a cast
    [-Wint-conversion]

Fixed in `07de44f` with a guarded include, pushed to `jerry73204/zenoh-pico`
`main`, pointer bumped here.

**The near-miss:** that commit was made hours earlier and then LOST — a
superproject checkout reset the submodule HEAD back to `61ed48f`, leaving
`07de44f` as a dangling object, the include absent from the working tree, and
this issue claiming a fix that was no longer in the tree. Nothing detected it:
the submodule pointer never moved, so `git status` in the superproject was clean.
A fork commit is not landed until the fork is PUSHED and the pointer bumped —
until then a routine checkout can silently revert it.

**Full vendored-C sweep (2026-08-03) — the class is confined to zenoh-pico.**

Two complementary methods, because neither alone covers the tree:

1. `scripts/dev/sweep-vendored-c-decls.sh` — `-fsyntax-only` per TU with both
   diagnostics as errors. Catches sites reachable without build context, which
   is where zenoh-pico's two lived.

   | tree | hits | compiled | needs build ctx |
   | --- | --- | --- | --- |
   | mbedtls | 0 | 159 | 7 |
   | micro-xrce-dds-client | 0 | 3 | 52 |
   | cyclonedds | 2 (both non-build) | 3 | 253 |
   | ThreadX | 0 | 17 | 2153 |

   The two cyclonedds hits are `docs/manual/_static/security_by_qos.c` (a manual
   snippet) and `etc/coverity-model.c` (a static-analysis model) — neither is
   compiled by any build. mbedtls is the only tree this method genuinely covers;
   for the rest most TUs need a generated config or a port header.

2. Building the `-sys` crates in their REAL configuration, which is the coverage
   method 1 cannot reach. On gcc >= 14 both diagnostics are errors by default,
   so a clean build on a current compiler proves the class absent for whatever
   configuration that build selects:

   ```
   CFLAGS="-Werror=implicit-function-declaration -Werror=int-conversion" \
       cargo build -p zpico-sys -p cyclonedds-sys -p nros-rmw-xrce-cffi
   ```

   All three build clean on gcc 16.1.1 — including `zpico-sys` itself, which is
   the regression check for the two fixes above.

Not swept: FreeRTOS/lwIP and NetX Duo are not provisioned on this host, so their
vendored C has never met a gcc >= 14. They are the remaining exposure.

**Original zenoh-pico sweep (unchanged):** `-fsyntax-only -Werror=implicit-function-declaration
-Werror=int-conversion` over all 128 host-compilable zenoh-pico TUs, in both the
shipped config and with every `Z_FEATURE_LINK_*` forced on, reports zero further
sites. The other vendored C (mbedtls, micro-xrce-dds-client, FreeRTOS/lwIP,
NetX) has still not been swept.
