# `nros setup` — toolchain & dependency management (W.5 design)

Status: design study · Date: 2026-05-28 · Tracks Phase 172 **W.5**

## Problem

First-image setup is the largest UX delta vs micro-ROS
(`docs/research/build-config-deploy-comparison.md`): a one-board user pays a
**workspace-wide, build-from-source** cost.

- **`just <module> setup` exists** (qemu, freertos, nuttx, threadx-linux, esp32,
  zenohd, cyclonedds, orin-spe, px4, …) — but it is a **contributor** surface:
  it requires `just`, and most recipes **clone + build from source**
  (`just qemu setup-qemu` *compiles* QEMU → the 2.7 GB `third-party/qemu`; SDK
  recipes `git clone` kernels and build them). `third-party/` ≈ **7.4 GB**.
- A **user** consuming nano-ros may not have `just`, wants **one board**, and
  should not compile a toolchain or QEMU to run a first image.

## Goal

A first-class **`nros setup <board>`** (no `just` needed) that fetches
**prebuilt** toolchains + deps, **board-scoped**, versioned + cached + pinned —
build-from-source only as a fallback. Model it on package/toolchain managers
that already solved this.

## What to learn from (survey)

| System | Mechanism | Lesson for `nros setup` |
|---|---|---|
| **Android SDK Manager** (`sdkmanager`) | versioned package repo (`repository2.xml`); `sdkmanager --install "platform-tools" "ndk;r26"`; prebuilt per host-OS; explicit `--licenses`; `$ANDROID_HOME` store | the core model: a **versioned package index**, prebuilt-per-host, **license acceptance**, a shared store — *never* builds from source |
| **PlatformIO** (`pio pkg` / `platform install`) | **board → platform → toolchain+framework** resolution; prebuilt packages per board; lockfile (`platformio.ini` pins) | **board-scoped resolution** + prebuilt toolchains — the closest analogue to `nros setup <board>` |
| **rustup** | channels + `target add` + components; prebuilt host artifacts; signed manifests | **granular targets/components** + signed/hashed manifests; we already depend on it for the Rust target |
| **espup** | downloads prebuilt xtensa/riscv GCC + LLVM for ESP | prebuilt **cross-toolchain** precedent for an MCU vendor |
| **Zephyr SDK installer** | prebuilt GNU cross-toolchains per arch, versioned tarballs + hashes | prebuilt cross-toolchain tarballs, host-arch matched |
| **west** | manifest-driven fetch (git/source) | the **manifest** concept — but source-based, the thing we want to avoid for toolchains |
| **Conda / vcpkg / conan** | binary package mgmt + lockfile + a channel/registry | **binary deps + lockfile**, reproducible resolution |

**Takeaway:** Android SDK Manager + PlatformIO are the template — a *versioned
package index* of *prebuilt* artifacts, resolved *per board/target*, with
*license gates* and a *lockfile*. west/`just setup` (source) is what we're
moving away from for the user path.

## Proposed model

### 1. A package index (manifest)

A checked-in (or fetched) `nros-sdk-index.toml` declares packages: versioned,
per-`(host_os, host_arch)` prebuilt URL + sha256, optional license gate.

```toml
[package.qemu]            # prebuilt QEMU — NOT a 2.7 GB source build
version = "11.0-nros1"
[package.qemu.dist.linux-x86_64]
url = "https://…/qemu-11.0-nros1-linux-x86_64.tar.zst"
sha256 = "…"
[package.qemu.dist.macos-arm64] # …

[package.arm-none-eabi-gcc]
version = "13.2"
# host-matched prebuilt from ARM's release page (redistributable)

[package.freertos-kernel]      # source-redistributable kernel: vendored tarball
version = "10.6.2"; kind = "source"   # unpacked, not built until cargo links it

[package.nv-spe-fsp]
version = "36.3"; license = "nvidia-sdk-manager"  # license-gated → instruct, don't fetch
```

### 2. Board → package resolution

A board descriptor (reuse `profile()` / the board crates) maps a board to its
package set:

```
nucleo_f767zi → { arm-none-eabi-gcc, freertos-kernel, lwip, qemu(optional), rmw=<sel> }
esp32-c3      → { esp-toolchain (via espup-style), esp-idf|baremetal, rmw }
native        → { host cc, zenohd(optional) }
```

`nros setup nucleo_f767zi` fetches **only** that set — not esp32+px4+….

### 3. Fetch → verify → cache → pin

- Download prebuilt artifacts (host-matched), verify sha256, unpack into a
  **shared store** (`$NROS_HOME` / `~/.nros/sdk/<pkg>/<version>/`, symlinked
  into `third-party/` for the build, or referenced by env). Shared across
  workspaces → fetch once.
- Write `nros-sdk.lock` (resolved versions + hashes) for reproducibility.
- **License-gated** packages (NVIDIA SPE, ARM FVP): never auto-download —
  print the install instruction + the expected env var (`NV_SPE_FSP_DIR`), and
  `nros doctor` already checks presence (Phase 172 deploy pin-check).
- **Source-only fallback:** when no prebuilt exists for a `(pkg, host)`, fall
  back to the existing `just <module> setup` source build, with a clear notice.

### 4. CLI surface

```
nros setup <board>            # board-scoped prebuilt fetch (the user path)
nros setup --target <triple>  # by target instead of board
nros setup --list             # available packages + versions (sdkmanager --list)
nros setup --licenses         # accept license gates
nros doctor                   # already reports SDK/pin presence
```

`nros build`/`nros deploy` gain a friendly error when a needed package is
missing: *"run `nros setup <board>`"* (mirrors today's *"run `nros metadata
--build`"* hints).

## Boundaries / non-goals

- **`just <module> setup` stays** the contributor / source-of-truth / CI path
  (it can become a thin caller of the same index). `nros setup` is the *user*
  path and is `just`-free.
- **Not** a general package manager — only nano-ros's toolchain/SDK/QEMU deps.
- **Prebuilt hosting** is an open logistics item (who hosts the QEMU/toolchain
  binaries; GitHub Releases is the likely default). Redistribution licensing of
  each artifact must be checked per package (QEMU GPL ok; vendor SDKs gated).

## Payoff

Turns the 7.4 GB / 20–60 min workspace-wide source build into a board-scoped
prebuilt fetch (target deps only, no QEMU compile) — closing the first-image
UX delta vs micro-ROS's `create_firmware_ws.sh <board>` / PlatformIO's
board-scoped toolchain install, without giving up the source path for
contributors.

## See also

- `docs/research/build-config-deploy-comparison.md` — the first-image
  time+space measurement that motivates this (W.5).
- `docs/development/sdk-tiers.md` — the current `just setup` tier model (the
  source-build baseline this supersedes for users).
- `docs/roadmap/phase-172-orchestration-deferred.md` — W.5 tracking.
