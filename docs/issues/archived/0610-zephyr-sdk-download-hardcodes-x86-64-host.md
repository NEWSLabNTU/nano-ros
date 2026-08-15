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

The artifact moved into the index, where host-keyed artifacts already live.
`[tool.zephyr-sdk]` carries a `dist.<host>` row per host and `nros setup --tool
zephyr-sdk --prefix scripts/zephyr/sdk` picks the one matching `host_key()`,
downloads it, verifies the sha256 and unpacks it. The shell script keeps only
the step the schema cannot express — running the SDK's own `setup.sh -t
<target>` to register toolchains.

A first pass fixed this in the script, selecting tarball and checksum together
from `uname -m`. That was correct and still the wrong place: `packages/cli`'s
own guidance is that board/toolchain/source knowledge belongs in
`nros-sdk-index.toml`, and leaving it in bash would have meant two spellings of
"which SDK does this host need" — the exact shape of the bug.

Two things about this entry differ from its neighbours, both deliberate and
both noted in the index:

- **The URLs are upstream, not repackaged** into `NEWSLabNTU/nano-ros-sdk`.
  Every other `dist.*` is a repack; this archive is 1.3 GiB and we apply
  nothing to it, so a repack would cost a release asset per host for no gain.
- **It unpacks to `<prefix>/zephyr-sdk-<version>/`**, not the `<prefix>/bin`
  layout the repacked dists share — which is exactly the path the script and
  `ZEPHYR_SDK_INSTALL_DIR` already expect, since `sdk_store.rs` runs `tar -xf`
  with no `--strip-components`.

Dropping out of the script with the fetch: the local download cache, the
`sha256sum` re-verification, and the `aria2c` prerequisite. That last one is a
real trade — aria2c pulled with 16 connections and `nros` uses curl, so a cold
fetch is slower. Accepted: a second spelling of the host mapping is what caused
this issue.

## Verified

`just zephyr setup` completes on aarch64 and `just zephyr doctor` reports west,
the Zephyr workspace and the `armv7a-none-eabi` Rust target present — that run
used the first (in-script) form of the fix, and it is what unblocked the zephyr
fixture family.

For the index form, the two things that differ from a normal dist were checked
directly rather than by re-downloading 1.3 GiB onto a host that already has the
SDK:

```
$ nros setup --tool zephyr-sdk --prefix /tmp/zsdk-probe --dry-run
nros setup --tool zephyr-sdk: prebuilt 0.16.8 (dist linux-arm64) → /tmp/zsdk-probe
```

and `tar -xf` on a synthetic `.tar.xz` unpacks to `prefix/zephyr-sdk-0.16.8/`,
confirming both that GNU tar auto-detects xz (the `zstd` preflight in
`sdk_store.rs` correctly does not fire for a `.xz` URL) and that nothing strips
the leading directory. The fetch itself is the shared code path every other
tool uses.

**Not yet exercised: a cold end-to-end provision on a host without the SDK.**
Worth doing once on a clean machine or with `--force`.

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
