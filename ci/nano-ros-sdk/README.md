# nano-ros-sdk (drop-in skeleton)

Prebuilt **host** toolchains/tools for nano-ros, hosted on this repo's GitHub
Releases. Phase 187.5.

This directory is the **drop-in skeleton** for the `NEWSLabNTU/nano-ros-sdk`
repo. The maintainer creates that repo and copies these files in; they are kept
here in the nano-ros tree only so they are reviewed alongside the index +
gate. Once the repo exists, this `ci/nano-ros-sdk/` copy can be deleted.

## What this repo hosts

Only **target-agnostic host tools** — the binary distribution matrix is just
`host-OS × host-arch`, not the target combinatorics:

| tool | source build / repackage | license |
|---|---|---|
| `qemu` | build from NEWSLabNTU/qemu fork (patched) | GPL ✓ |
| `arm-none-eabi-gcc` | repackage ARM upstream | GPL ✓ |
| `riscv-none-elf-gcc` | repackage upstream | GPL ✓ |
| `zenohd` | `cargo install` from zenoh 1.7.2 | Apache/EPL ✓ |
| `openocd` | build from upstream | GPL ✓ |

Libraries + apps (FreeRTOS, lwIP, ThreadX, zenoh-pico, the user's nodes) are
**not** hosted — they build with the app for the user's chosen target. Vendor
SDKs that forbid redistribution (NVIDIA SPE, ARM FVP) are **never** hosted; they
stay `[gated.*]` in the index.

## Release layout (the contract `nros setup` reads)

```
tag    = <tool>-<version>          e.g. qemu-11.0-nros1
asset  = <tool>-<host>.tar.zst     e.g. qemu-linux-x86_64.tar.zst
         <tool>-<host>.tar.zst.sha256
```

The tarball unpacks to the install prefix's contents (`bin/`, `lib/`, …) so
that `$NROS_HOME/sdk/<tool>/<version>/` is identical whether `nros setup`
fetched the prebuilt or built from the index's `[tool.*.source]` recipe.

## Building / seeding

```
gh workflow run build-tool.yml -f tool=qemu -f version=11.0-nros1
```

`build-tool.yml` runs the host matrix, each runner calls
`scripts/build-<tool>.sh <version> <host-key>` (must produce
`dist/<tool>-<host>.tar.zst`), then publishes the Release. After seeding, fill
the matching `dist.<host> = { url, sha256 }` in nano-ros's `nros-sdk-index.toml`
— the `sdk-index-gate` CI there verifies it before merge.

## host keys

`linux-x86_64` · `linux-arm64` · `macos-arm64` (must match
`nros-cli-core::orchestration::sdk_index::host_key`).

Linux runners are **Ubuntu 22.04** (ROS 2 Humble baseline). A 24.04 runner for
Jazzy is added later when Jazzy support lands.
