---
rfc: 0014
title: "`nros setup` — toolchain & dependency management (W.5 design)"
status: Stable
since: 2026-05
last-reviewed: 2026-05
implements-tracked-by: []
supersedes: []
superseded-by: null
---

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

## Distribution: GitHub Releases, **host toolchains only** (no registry server)

Two scoping decisions make this affordable with **no infrastructure**:

**(a) Prebuild only host *toolchains / tools*, never libraries or apps.**
- **Prebuilt** = the host-side, **target-agnostic** tools that are expensive to
  build: QEMU, the cross-compilers (`arm-none-eabi-gcc`, riscv, xtensa),
  `zenohd`, `openocd`. A cross-gcc *runs on the host* and *targets* any MCU — so
  the matrix is just **host-OS × host-arch** (≈ linux-x86_64, linux-arm64,
  macos-arm64, … — a handful), **not** per-target. Small, finite, buildable in
  CI once per release.
- **Source** = nano-ros libs (`nros-*`), the **user's app**, and the
  target-compiled kernels/libs (FreeRTOS, zenoh-*pico*, the C glue). The
  toolchain compiles these for **whatever platform × arch the user picks** —
  that combination is the user's choice and is *combinatorial*, so it stays
  source. This sidesteps the precompiled-matrix problem entirely: we never ship
  a `target × arch × RTOS × RMW` binary.

**(b) Host the prebuilts on GitHub Releases.** No registry server to run or pay
for. A release tag (e.g. on a `nano-ros-sdk` repo, keeping the main repo's
releases clean) carries one asset per `(tool, host)`; GitHub serves them free
(public repos, fair-use bandwidth; 2 GB/asset limit — a slim QEMU/toolchain is
well under). URLs are stable: `…/releases/download/<tag>/<asset>`. A CI matrix
builds/repackages the tools per host and uploads them.

**License boundary:** only *redistributable* tools are hosted — QEMU (GPL),
cross-GCC (GPL), `zenohd` (Apache/EPL), OpenOCD (GPL). Vendor SDKs that forbid
redistribution (NVIDIA SPE, ARM FVP) are **never** hosted — they stay
license-gated (`--licenses` / instruct + `nros doctor` presence check).

## Proposed model

### 1. A package index (manifest committed in-repo → GitHub Release assets)

`nros-sdk-index.toml` is **checked into the repo** (no fetch of a remote index);
it pins each prebuilt tool to a GitHub Release asset URL + sha256, per host.
Source packages carry no `dist` (they build from a vendored tarball / submodule).

```toml
# --- prebuilt HOST TOOLS (GitHub Releases) — target-agnostic ---
# Each tool declares BOTH a prebuilt `dist` (per host) AND a `source` recipe.
# Either path installs into the SAME prefix → identical layout downstream.
[tool.qemu]                       # NOT a 2.7 GB source build
version = "11.0-nros1"
dist.linux-x86_64 = { url = "https://github.com/<org>/nano-ros-sdk/releases/download/qemu-11.0-nros1/qemu-linux-x86_64.tar.zst", sha256 = "…" }
dist.macos-arm64  = { url = "…", sha256 = "…" }
# Fallback when no `dist` matches the host: build from source into the same prefix.
[tool.qemu.source]
git = "https://github.com/<org>/qemu"; ref = "v11.0-nros1"
# build/install must land in $NROS_HOME/sdk/qemu/11.0-nros1/  (the layout contract)
configure = "./configure --prefix={prefix} --target-list=arm-softmmu,riscv64-softmmu"
install   = "make -j && make install"

[tool.arm-none-eabi-gcc]          # cross-compiler: runs on host, targets any Cortex-M/R
version = "13.2"
dist.linux-x86_64 = { url = "…/releases/download/arm-gcc-13.2/arm-gcc-linux-x86_64.tar.zst", sha256 = "…" }

[tool.zenohd]                     # host router binary
version = "1.0.0"
dist.linux-x86_64 = { url = "…", sha256 = "…" }

# --- SOURCE packages — compiled by the toolchain for the user's chosen target ---
[source.freertos-kernel]  version = "10.6.2"   # vendored tarball, built with the app
[source.zenoh-pico]       version = "1.0.0"    # small C lib, built with the app

# --- license-gated: never hosted, instruct only ---
[gated.nv-spe-fsp]  version = "36.3"  env = "NV_SPE_FSP_DIR"  installer = "nvidia-sdk-manager"
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

### 3. The install-layout contract (prebuilt and source produce the *same* layout)

A tool always lands at one **versioned prefix**, regardless of how it got there:

```
$NROS_HOME/sdk/<tool>/<version>/    # = {prefix}
    bin/  lib/  share/ …            # standard install tree
    .nros-provenance               # "prebuilt" | "source", + sha256
```

- **Prebuilt path:** download the host-matched `dist` tarball, verify sha256,
  unpack into `{prefix}`.
- **Source fallback** (no `dist` for this host): clone `[tool.<x>.source].git`
  @ `ref`, run `configure`/`install` with `--prefix={prefix}` (or copy the
  built tree in) — it **installs into the identical `{prefix}`**. Borrowed from
  Homebrew (bottle-or-build-from-source → same Cellar path) and Nix (versioned
  store prefix).
- **Layout-stable downstream:** `build.rs` / the deploy runner / `nros doctor`
  resolve `$NROS_HOME/sdk/qemu/11.0-nros1/bin/qemu-system-arm` — they never know
  or care whether it was fetched or built. `.nros-provenance` records which, for
  diagnostics only.
- The store is **shared** across workspaces/clones (one copy per
  `(tool, version)`), symlinked/`-L`'d into the build or referenced by env.
- **License-gated** tools (NVIDIA SPE, ARM FVP): never fetched **or** built —
  print the installer instruction + expected env var (`NV_SPE_FSP_DIR`);
  `nros doctor` checks presence (Phase 172 deploy pin-check).

### 4. Version management — files map to GitHub assets

- **The index is the version SSOT.** `nros-sdk-index.toml` (committed) pins each
  tool's `version`. That version maps **deterministically** to the GitHub
  artifact: release **tag** `= <tool>-<version>`, asset `= <tool>-<host>.tar.zst`
  — so `nros setup` computes the asset from `(tool, version, detected host)` and
  fetches the corresponding file; the explicit `url` + `sha256` in the index pin
  integrity (and let a tool point elsewhere when needed).
- **The lockfile records what's installed.** `nros-sdk.lock` (committed per
  workspace) captures the resolved `(tool, version, sha256, provenance)` actually
  in the store — index = *desired*, lock = *installed*, like Cargo.lock. A clone
  with the lock reproduces the exact toolchain set.
- **Bumping a tool:** edit `version` in the index → CI builds the new per-host
  assets under the new tag → users get it on the next `nros setup` (or stay on
  the locked version until they update). The source `ref` bumps in lockstep, so
  the fallback build matches the prebuilt version.
- The same version files drive **both** `nros setup` (user, prebuilt-first) and
  `just <module> setup` (contributor, source) — one source of truth for "which
  QEMU / which arm-gcc", no drift between the two paths.

### 5. CLI surface

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

## Release workflow — a bumped version is always available

The hard guarantee: **an index version is merged only after its prebuilt assets
exist + verify on GitHub.** A version can never point at a missing asset. The
mechanism mirrors conda-forge / Homebrew bottles — *CI builds the artifacts and
writes their hashes back into the PR; merge is gated on the build matrix going
green.*

### Versioning method

- **Version string** `= <upstream>-nros<rev>` (e.g. `qemu 11.0-nros1`,
  `arm-none-eabi-gcc 13.2-nros1`). The `-nros<rev>` suffix lets *repackaging*
  (a patch, a `configure` change) bump independently of the upstream version.
- **GitHub mapping is deterministic:** release **tag** `= <tool>-<version>`,
  assets `= <tool>-<host>.tar.zst` (+ `.sha256`). So the index `version` alone
  determines the fetch URL for any host.
- **Three files, one truth:** `nros-sdk-index.toml` (desired version + per-host
  sha256) → CI builds → `nros-sdk.lock` (installed). The per-host **sha256 in
  the index is itself the availability proof** — CI computed it from the real
  uploaded asset.

### Bump → release flow (the gate)

```
contributor edits nros-sdk-index.toml:  [tool.qemu].version "11.0-nros1" → "11.1-nros1"
        │   (sha256 fields blanked / marked TODO)
        ▼  PR opened
┌─ CI: build-matrix (one job per host: linux-x86_64, linux-arm64, macos-arm64, …) ─┐
│  1. build from [tool.qemu.source] @ ref  — the SAME recipe nros setup's source   │
│     fallback uses, so prebuilt ≡ source-built                                    │
│  2. upload <tool>-<host>.tar.zst to a DRAFT release  tag=qemu-11.1-nros1          │
│  3. compute sha256; commit it back into the PR's index (bot auto-commit)         │
└──────────────────────────────────────────────────────────────────────────────────┘
        ▼
   required check `sdk/<tool>`:  for every host the index declares a dist for,
   the asset downloads + sha256 matches.   RED ⇒ cannot merge.
        ▼  merge
   CI promotes the draft release → published.  Assets are now the stable URLs
   the (now-merged) index points at, with matching hashes.
```

Outcome:
- A bumped version is in `main` **iff** its assets are live + hash-verified — so
  every user `nros setup` finds them. No dangling versions.
- **Hosts CI didn't build** simply have no `dist.<host>` entry → `nros setup`
  uses the `[tool.X.source]` fallback (same `ref`, identical `{prefix}` layout).
  Availability is still guaranteed: prebuilt where built, source-built where not,
  never a 404.
- **Source recipe is CI-tested** (at least one host builds from source each run)
  so the fallback is known-good, not aspirational.
- **Rollback** is trivial: revert the index edit → the old tag/assets still
  exist; the lock already pinned the old hash.

A contributor's checklist becomes: bump `version` in the index, open the PR, let
the build matrix fill in the hashes + publish — they never hand-upload or
hand-hash. The same `[tool.X.source]` recipe powers both the CI prebuild and the
user source fallback, so the two paths can't diverge.

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
- **Not** a general package manager — only nano-ros's host toolchains/tools.
- **Only host toolchains/tools are prebuilt.** Libraries + the user's app +
  target-compiled kernels stay **source** — the final platform × arch is the
  user's choice, so we never ship that combinatorial binary matrix.
- **Hosting = GitHub Releases** (decided — no registry server). A CI matrix
  builds/repackages the redistributable tools per host and uploads; the index is
  committed in-repo pointing at the Release assets. Vendor SDKs that forbid
  redistribution are never hosted (gated). Open item: which org/repo holds the
  Release assets (`nano-ros-sdk` repo suggested) + the CI build matrix.

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
