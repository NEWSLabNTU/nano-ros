---
id: 610
title: "`just zephyr setup` downloads the x86_64 Zephyr SDK on any host, and
  fails 1.3 GiB later with a message naming neither the arch nor the tarball"
status: resolved
type: bug
area: build
related: [issue-0582, issue-0466, issue-0603]
---

## Symptom

`just zephyr setup` on an aarch64 Ubuntu host downloads for several minutes,
verifies its checksum, extracts, and then:

```
[INFO] Extracting SDK to scripts/zephyr/sdk...
[INFO] Running SDK setup...
[INFO]   Toolchains: x86_64-zephyr-elf arm-zephyr-eabi
Zephyr SDK 0.16.8 Setup

Installing host tools ...
ERROR: Host tools installation failed
error: recipe `setup` failed with exit code 30
```

Nothing in that output says "wrong architecture". The download succeeded, the
checksum PASSED, and the failure surfaces from inside the SDK's own
`setup.sh`. It reads as a broken or incompatible SDK release.

The tell is one line earlier in the log:

```
Download complete: .../zephyr-sdk-0.16.8_linux-x86_64.tar.xz
```

on a machine where `uname -m` is `aarch64`.

## Mechanism

`scripts/zephyr/setup.sh` hardcoded both the tarball and its checksum:

```sh
ZEPHYR_SDK_TARBALL="zephyr-sdk-${ZEPHYR_SDK_VERSION}_linux-x86_64.tar.xz"
ZEPHYR_SDK_SHA256="cb4e4012751e4526aaf1ec1e8ab9b4ded5681e2e01711b64f7a1b519ff7dbc6a"
```

The SDK tarball is keyed on the HOST architecture. Its cross toolchains
(`arm-zephyr-eabi` &c) target boards and are the same everywhere, but the
**host tools** inside it — the binaries the SDK's installer registers — are
native executables for the host it was built for. So on aarch64 the fetch and
the checksum both succeed and the install cannot.

This is the failure mode issue 0582 catalogued: a value meaning "this machine"
written as an x86 literal, invisible on x86, and silent-ish everywhere else.
Here it is worse than usual in one respect — the checksum *passing* actively
argues the download was correct, so the natural next suspicion is the SDK
release rather than the request.

Upstream ships the arch we need; nothing had to change but the request:

```
$ curl -sL .../v0.16.8/sha256.sum | grep linux-aarch64
83782b4cf595bb3da8a6c7c1ade01eed00ad03f8ba0c72da6680693192b3668d  zephyr-sdk-0.16.8_linux-aarch64.tar.xz
```

## Fix

Select tarball and checksum together from `uname -m`, with an unmapped
architecture as a hard error naming what to add rather than a silent default to
x86_64. Both checksums are the upstream ones from the release's `sha256.sum`;
the x86_64 value is byte-identical to the one that was hardcoded, which
confirms the source.

Keeping the two in one `case` is deliberate: a tarball and a checksum that can
drift apart is how you get a confusing verification failure later, and the pair
is the unit that actually varies.

## Verified

`just zephyr setup` completes on aarch64, and `just zephyr doctor` reports west,
the Zephyr workspace and the `armv7a-none-eabi` Rust target all present.

## How it was reached

`just ci-matrix` (tier 2) stopped at its lane gate on three missing zephyr
compile-check fixtures (`west_bringup_zephyr`, `west_board_import`,
`zephyr_self_pkg_rust`), because the zephyr fixture family had reported
`SKIPPED (west not found)`. Provisioning west is what surfaced this.

Note for anyone reading fixture logs: an earlier tier-2 run in the same session
reported `== zephyr == OK` on this host with no west installed. That run
predated ~25 upstream commits which changed the zephyr lanes; the "OK" was a
lane with nothing to do, not evidence that west was ever present. A family
reporting OK is not proof its toolchain exists.
