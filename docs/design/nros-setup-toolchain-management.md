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

### How Android's SDK Manager works (the package-index model)

- **A hosted, versioned package repository.** Google publishes XML manifests
  (`repository2-1.xml`, sys-image/addon lists) describing every package: a
  path-like **id** + a **revision** (version) + **per-host archives**
  (`linux`/`macosx`/`windows`) each with a URL, **checksum**, and size. The
  client reads the manifest, never a directory listing.
- **Path-like, versioned package ids.** `platform-tools`,
  `platforms;android-34`, `build-tools;34.0.0`, `ndk;26.1.10909125`,
  `cmake;3.22.1`, `system-images;android-34;google_apis;x86_64`, `emulator`.
  The `;` segments namespace + pin a version.
- **CLI verbs.** `sdkmanager --list` (installed + available), `sdkmanager
  --install "platforms;android-34" "build-tools;34.0.0"`, `--update`,
  `--uninstall`, **`--licenses`** (accept the gated SDK licenses, cached under
  `licenses/`). Everything is **prebuilt** — it downloads compiled binaries,
  never builds.
- **A shared, fixed-layout store.** `$ANDROID_HOME` holds `platform-tools/`,
  `platforms/android-34/`, `build-tools/34.0.0/`, `ndk/26.x/` — shared across
  all projects, so a package is fetched once.
- **Build declares, manager provides.** Gradle's Android plugin names what it
  needs (`compileSdk = 34`, `ndkVersion = …`); a missing package is an error
  pointing at `sdkmanager` (and AGP can auto-trigger the install after license
  accept). The build never carries the toolchain.

→ nano-ros borrows: the **`nros-sdk-index.toml`** ≈ `repository2.xml`; `nros
setup --list/--install/--licenses` ≈ the sdkmanager verbs; `$NROS_HOME/sdk/`
≈ `$ANDROID_HOME`; license gates for NVIDIA SPE / ARM FVP ≈ `--licenses`.

### How PlatformIO works (the board-scoped resolution model)

- **Board-centric config.** `platformio.ini` declares
  `board = nucleo_f767zi`, `platform = ststm32`, `framework = arduino|zephyr`.
- **board → platform → packages.** A **board manifest** (JSON) names the MCU +
  the **packages** that board needs; installing the platform pulls them, all
  **prebuilt + versioned**: the **toolchain** (`toolchain-gccarmnoneeabi`), the
  **framework** (`framework-arduinoststm32`, `framework-zephyr`), and
  upload/debug tools (`tool-openocd`, `tool-stlink`). Board→deps is *data*, not
  a script.
- **A registry + semver pins.** `registry.platformio.org` hosts packages;
  `platformio.ini` pins (`platform = ststm32@~17.0.0`). `pio pkg list`,
  `pio pkg install`.
- **Shared cache + lazy install.** Packages land in `~/.platformio/packages/`
  + `platforms/`, shared across projects. There is usually **no explicit setup
  step** — the first `pio run` for a new board **auto-installs** the platform +
  toolchain + framework from the board declaration, then builds.

→ nano-ros borrows: the **board→package-set resolution** (reuse `profile()` /
the board crates as the "board manifest"), the **shared cache**, and the
**auto-install-on-build** ergonomic — `nros build`/`nros deploy` triggering a
missing `nros setup <board>` the way `pio run` triggers the platform install.

**Split of roles:** Android gives the *index + CLI + license + shared-store*
shape; PlatformIO gives the *board-scoped resolution + auto-install-on-build*
ergonomic. `nros setup` = Android's package management **+** PlatformIO's
board-centric, lazy resolution.

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

## Does PlatformIO host a binary registry? (yes — and how)

Yes. `registry.platformio.org` is a typed package registry — **`tool`**
(toolchains, `tool-openocd`, uploaders/debuggers, even `tool-qemu-*`),
**`library`**, and **`platform`** (board-support meta-packages). Each package
version carries **per-host archives** (a `system` field: `linux_x86_64`,
`darwin_arm64`, `windows_amd64`, …) — prebuilt tarballs with a `package.json`
manifest (name, version, system, deps). Anyone publishes via `pio pkg publish`;
the toolchains are mostly *repackaged upstream* binaries (ARM GCC, Espressif
xtensa, …). Binaries are served from PlatformIO's dl/CDN, fetched on demand,
and cached **once** in a shared global store (`~/.platformio/packages/`), deduped
by name+version+system. So: a hosted, versioned, per-host **binary** registry +
a shared on-demand cache — exactly the shape `nros setup` wants.

## Reducing space bloat

Measured composition of today's 7.4 GB `third-party/` (the thing W.5 attacks):

| dir | size | what it is | fix |
|---|---|---|---|
| **qemu** | **2.7 GB** | source clone **+ a 1.4 GB compiled `build/` tree** | **prebuilt QEMU binary** (~30–80 MB) → −~2.6 GB |
| **zenoh** | 813 MB | full Zenoh (router) source/build | **prebuilt `zenohd`** release binary → −~0.8 GB (zenoh-*pico* is small + builds with the app) |
| esp32 | 1.4 GB | ESP-IDF source tree | **board-scoped** — only when targeting esp32 |
| px4 | 1.2 GB | PX4 source repo | board-scoped — vendor-module only |
| nuttx / threadx | 655 / 389 MB | RTOS kernel source | board-scoped + redistributable tarball (no build) |

`.git` is already tiny across all of them (they shallow-clone) — **git history
is not the bloat; building host tools from source is.** The levers, biggest
first:

1. **Prebuilt host tools — the dominant win.** QEMU (−2.6 GB) and `zenohd`
   (−0.8 GB) are *host* tools currently built from source. Fetch prebuilt
   binaries (QEMU 11.0 release / system package; `zenohd` from Zenoh's release
   page). ~3.4 GB → ~0.1 GB without touching board scoping. This is what
   PlatformIO does (`tool-qemu-*`, prebuilt uploaders).
2. **Board scoping.** A Nucleo user pulls FreeRTOS (30 MB) + a prebuilt
   arm-none-eabi-gcc (~0.3 GB) + prebuilt QEMU (~50 MB) ≈ **0.4 GB**, not
   esp32+px4+nuttx+threadx. The single biggest *multiplier*.
3. **Shared, deduped store.** `~/.nros/sdk/<pkg>/<ver>/` shared across
   workspaces + clones (symlinked into the build), one copy per
   (pkg, version, host) — a dev with 3 checkouts pays once, not 3×.
4. **No build trees.** Prebuilt artifacts carry no intermediate objects (the
   1.4 GB QEMU `build/` vanishes); the source fallback must `clean` after.
5. **Slim the toolchains.** Prebuilt cross-toolchains can drop docs/examples/
   unused multilibs (Zephyr-SDK / PlatformIO ship slimmed variants).

**Net:** prebuilt host tools (#1) + board scoping (#2) take the *one-board*
first-image footprint from **7.4 GB → ~0.4 GB**, and the shared store (#3)
keeps it amortized.

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
