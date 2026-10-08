---
id: 1757
title: "The module's image Kconfig fragment states `rmw = \"zenoh\"` LAST, so a Zephyr leaf's XRCE variant builds zenoh-pico and fails on `struct addrinfo`"
status: open
type: bug
area: [zephyr, build, ci]
severity: high
found: 2026-10-08
related: [1721, 1712]
---

## Measured

`nightly` run **37731321432** on `main` @ `ad31072d5` (schedule, 2026-10-08
05:13Z), job **113160842757** `tier 2 nightly (pairwise cover)`, runner
`nano-ros-runner`, step `just build tier2-nightly`. This is the first tier-2
nightly since the runner restart to get past `_require-lane-router`. The two
nights before it stopped at `provision-zenohd` exit 78, so they reached no
build.

```
== zephyr == FAILED (rc=2)
.../zpico-sys/zenoh-pico/src/system/zephyr/network.c:278:21: error: storage size of 'hints' isn't known
.../zpico-sys/zenoh-pico/src/system/zephyr/network.c:302:34: error: invalid use of undefined type 'struct addrinfo'
FATAL ERROR: command exited with status 1: /usr/bin/cmake --build .../build/zephyr-workspace-builds/3.7/build-c-service-server-xrce
  ✗ Zephyr fixture build FAILED (exit 2) — judging the tier-priority
    plan over the 15 of 32 image(s) that DID build
```

An **XRCE** image compiled **zenoh-pico**. The 15 images that built include
every `*-zenoh` leaf and the six `build-cpp-*-cyclonedds` leaves. The run log
names only the first failure, so which of the other 16 failed, and why, is
not established. `just ci matrix-nightly` was skipped, so no cell ran.

## Cause

Introduced by `3fa50123a` (phase-481 W1/W2, landed 2026-10-07). The Zephyr
module's `module_ext_root` hook (`zephyr/cmake/nros_image_kconfig.cmake`) runs
`nros ws leaf-system --kconfig-out`. That renders the image's `rmw` as
`CONFIG_NROS_RMW_<X>=y` (`cmd/leaf_kconfig.rs`, the `rmw_sym` row). The hook
then puts the fragment **last** in `EXTRA_CONF_FILE`, which is after every
conf file on every road. That ordering is deliberate: it is how W1 makes
`system.toml` win.

Every standalone Zephyr example leaf still states one image, and it says
`rmw = "zenoh"`. For example, `examples/zephyr/c/service-server/system.toml`
reads `[system] rmw = "zenoh"`. The same leaf builds its XRCE and Cyclone
variants from `prj-xrce.conf` / `prj-cyclonedds.conf`, which set
`CONFIG_NROS_RMW_XRCE=y` / `CONFIG_NROS_RMW_CYCLONEDDS=y`. Those confs now
lose the choice to the fragment's `CONFIG_NROS_RMW_ZENOH=y`. The phase doc
recorded the disagreement before W1 landed (§ "Why": "`c/talker/system.toml`
says `rmw = "zenoh"` while the same leaf builds XRCE and Cyclone variants").
W3, one image per RMW selected by `-DNROS_IMAGE`, is what resolves it, and W3
has not landed.

W1's acceptance measured only zenoh images:
`NROS_ZEPHYR_FIXTURE_FILTER='build-(c|rust)-(talker|listener)-zenoh|build-ws-c-entry-zenoh'`.
Those are exactly the images whose leaf conf and fragment already agree. No
merge-gating lane builds a Zephyr XRCE or Cyclone fixture, so nothing caught
it before the nightly.

**Worse than the red build, unverified.** An XRCE-variant image whose
`prj-xrce.conf` happens to supply what zenoh-pico needs would BUILD as a
zenoh image under an `-xrce` name. A Cyclone variant is the likelier case,
since it enables the POSIX socket API. That image would then fail at runtime
against an XRCE agent or a DDS peer, or pass a cell that checks only "it
published". Before believing any Zephyr XRCE or Cyclone cell result built
after `3fa50123a`, read its `build/.../zephyr/.config` for which
`CONFIG_NROS_RMW_*` is set.

## What it is NOT

- **Not the stale cross-checkout build dir.** This is not the
  `build-cortex-m-c-talker-zenoh` dir that issue 1387's gate flagged after the
  runner restart. The failing dir is `build-c-service-server-xrce`, and the
  error is a compile error in a source file, not a cached path.
- **Not a zenoh-pico regression.** `0f4088353` moved the pin on 2026-10-07, but
  `network.c`'s `getaddrinfo` path is unchanged. It is reached here only
  because the image selects zenoh without the POSIX Kconfig that every
  `prj-zenoh.conf` carries (`CONFIG_POSIX_API=y`, which `prj-xrce.conf` does not set).
- **Not issue 1721.** That issue is about `[image.<id>] env` reaching
  nothing; this one is about the RMW row reaching too much.

## What would close it

Either of these, measured by building one XRCE and one Cyclone Zephyr fixture
from each language (`NROS_ZEPHYR_FIXTURE_FILTER='build-(c|cpp|rust)-talker-(xrce|cyclonedds)'`)
and reading each `.config`'s `CONFIG_NROS_RMW_*`:

1. **W3's shape.** Each multi-RMW leaf states one image per RMW
   (`[image.zephyr_xrce]` …), and the fixture rows pass `-DNROS_IMAGE`.
2. **An interim refusal.** The renderer, or the hook, omits the RMW row when
   the leaf's own conf list already selects a different backend, or refuses
   loudly naming both. It must never let the fragment silently win a choice
   the conf list made.

A regression guard belongs with the fix. A merge-gating lane needs one cheap
check that a `*-xrce` fixture's rendered fragment does not state
`NROS_RMW_ZENOH`. A configure-only probe suffices; it needs no full build.
