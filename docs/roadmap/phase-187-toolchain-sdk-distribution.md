# Phase 187 — Toolchain & SDK distribution (`nros setup`)

**Goal.** Let a *user* deliver and run a first image board-scoped, prebuilt,
and `just`-free — without today's workspace-wide, build-from-source SDK setup
(`just setup` ≈ 7.4 GB incl. a 2.7 GB QEMU *source* build). Provide a
first-class `nros setup <board>` that fetches **prebuilt host toolchains/tools**
from a versioned index hosted on GitHub Releases, with a deterministic
source-build fallback that produces the identical install layout.

**Status.** Not started — design approved (split out of Phase 172 W.5,
2026-05-28).

**Priority.** P2 — first-image onboarding UX; no MVP capability depends on it,
but it is the largest first-image delta vs micro-ROS
(`docs/research/build-config-deploy-comparison.md`).

**Depends on.** Phase 172 (the deployment model, board `profile()` descriptors,
the `nros` CLI). **Design:** `docs/design/nros-setup-toolchain-management.md`
(lands on `main` with the Phase 172 merge; this doc forward-references it).

## Overview

micro-ROS's onboarding is **board-scoped** (`create_firmware_ws.sh <board>`
fetches one board's deps ≈ 0.5 GB; the Arduino path is a 14–22 MB precompiled
lib). nano-ros's `just setup` is a **workspace-developer** action pulling every
platform SDK and *building* QEMU from source — ~7.4 GB, 20–60+ min, even for a
one-board user, and it needs `just`. Phase 187 closes that gap.

The model (Android `sdkmanager` + PlatformIO):
- **Prebuild only host toolchains/tools** (QEMU, cross-GCC, `zenohd`, OpenOCD) —
  they run on the host and target any MCU, so the matrix is just
  `host-OS × host-arch` (small, finite, CI-buildable). **Libraries + the user's
  app + target-compiled kernels stay source** — the final platform × arch is the
  user's choice (combinatorial), compiled by the toolchain.
- **Host on GitHub Releases** — no registry server. A committed package index
  pins versions → Release assets; a CI bump→release gate guarantees a version is
  merged only after its assets exist + hash-verify.

## Architecture

- **Package index** (`nros-sdk-index.toml`, committed): `[tool.*]` (prebuilt
  `dist` per host **+** a `source` recipe), `[source.*]` (built with the app),
  `[gated.*]` (license-gated, never hosted). Version is the SSOT; maps
  deterministically to the GitHub tag/asset.
- **Board → package resolution:** reuse `profile()` / the board crates as the
  "board manifest"; `nros setup <board>` fetches only that board's set.
- **Install-layout contract:** every tool lands at
  `$NROS_HOME/sdk/<tool>/<version>/` whether fetched (unpack `dist`) or
  source-built (`--prefix={prefix}`) — downstream resolves the prefix,
  provenance-agnostic. Shared/deduped across workspaces; `nros-sdk.lock` pins
  installed.
- **Release workflow:** CI per-host build matrix → draft Release → sha256
  back-committed to the PR → required `sdk/<tool>` check → publish on merge.

## Work items

- [x] **187.1 — Package index format + loader.** DONE (codegen `fae5688`).
      `orchestration::sdk_index` — `SdkIndex` (`[tool]`/`[source]`/`[gated]`,
      per-host `dist` + `[tool.*.source]` recipe, version + sha256), `load`/
      `parse`, `ToolPackage::dist_for`/`installable_on` (prebuilt-or-source),
      `host_key()` (`<os>-<arch>`); `deny_unknown_fields` throughout; 4 tests.
      The committed `nros-sdk-index.toml` (real URLs+hashes) lands with 187.5
      (asset hosting). Format + loader only — board resolution/fetch are
      187.2–187.3.
- [x] **187.2 — `nros setup` CLI + board resolution.** DONE (codegen `0583eac`).
      `cmd/setup.rs`: `nros setup [board] [--target] [--list] [--licenses]
      [--index]`; `resolve_packages(board, target)` maps a board → its SDK
      package set (cross-toolchain by arch, qemu for sim boards, RTOS kernel
      sources, host `zenohd`, gated vendor SDKs); prints the per-host install
      plan (prebuilt dist / source-build fallback / gated / not-in-index) via
      `disposition`. Wired into `Cmd`/dispatch. 2 tests. *CLI + resolution +
      plan only — the fetch/source-build/cache/lockfile is 187.3.*
- [x] **187.3 — Fetch / verify / cache / source-fallback / lockfile.** DONE
      (codegen `2cc8891`). `orchestration::sdk_store`: `store_root`/`tool_prefix`
      (`$NROS_HOME/sdk/<tool>/<ver>/`), `Provenance` (`.nros-provenance`), `SdkLock`
      (`nros-sdk.lock` load/record/save), `plan_install`
      (Present/Prebuilt/Source/Unavailable, idempotent via the marker), `execute`
      (curl + sha256sum/shasum verify + tar for `dist`; git clone @ ref +
      `configure({prefix})`/install for source — same prefix either way). `nros
      setup` installs the resolved tools + writes the lock, or `--dry-run` plans.
      4 store tests; e2e-checked via `nros setup <board> --dry-run`/`--list`. The
      real fetch/build runs once a committed index exists (187.5).
- [x] **187.4 — Versioning + CI bump→release gate.** `verify-index.py` +
      `.github/workflows/sdk-index-gate.yml`: a PR touching `nros-sdk-index.toml`
      must have every prebuilt `dist` live + sha256-correct on the nano-ros-sdk
      Releases (read-only, no token). Verified it passes a filled index and
      fails an unreachable/wrong-hash dist. *Maintainer-owned remainder:* wire
      the `sdk-index-gate` check into branch protection.
- [x] **187.5 — `nano-ros-sdk` hosting repo + prebuilt builders.**
      `NEWSLabNTU/nano-ros-sdk` seeded (`build-tool.yml` host matrix:
      ubuntu-22.04 / -arm / macos-14). Five host tools built + published across
      all 3 hosts (15 assets), `dist` filled + gate-verified:
      `qemu-11.0.0-nros1` + `openocd-0.12.0-nros1` (source builds),
      `zenohd-1.7.2-nros1` + `arm-none-eabi-gcc-13.2-nros1` (ARM 13.2.rel1) +
      `riscv-none-elf-gcc-14.2-nros1` (xPack 14.2.0-3) (repackages). xtensa/ESP
      toolchain still pending (no builder yet). `ci/nano-ros-sdk/` is the
      review copy of the repo's seed; deletable now the repo is live.
- [ ] **187.6 — Unify with `just setup` + auto-install-on-build.** `just
      <module> setup` becomes a thin caller of the same index (one truth, no
      drift); `nros build`/`deploy` trigger a missing `nros setup <board>` (the
      PlatformIO lazy-install ergonomic).
  - [x] **Lazy auto-install** (`92d15f9`): `setup::ensure_tools(board, target,
        workspace)` — `nros build` (native) + `nros deploy` install the board's
        `[tool.*]` from the index into the store before building (prebuilt or
        source), warn-not-fail on unavailable, no-op away from a workspace /
        under `NROS_NO_AUTO_SETUP`. e2e deploy tests set that env to stay
        hermetic.
  - [x] **Method A — `nros` resolves + injects env for children** (`99a7c79`).
        The build tool is the single resolver (PlatformIO/Gradle model — best
        UX): `ensure_tools` returns the resolved tools' store `bin/` dirs;
        `activate_store_path` prepends them to the process `PATH` in `nros
        build`/`deploy`, so every child it spawns (cmake / cargo / west /
        `build[]` / `package[]`) finds the toolchain — cmake `find_program`,
        west, cross-gcc all honour `PATH`. The user never manages `PATH`; no
        subshell. **Non-`nros` scripts & code do NOT resolve the SDK path** — the
        harness, justfile recipes, cmake assume the SDK is *given* and only
        **check + warn** (the store-probe was reverted on this principle;
        check+warn lives in `just <plat> doctor` / `nros doctor`).
        **Host prebuilt unavailable:** `dist` missing → build from
        `[tool.*.source]` (187.3, same prefix); no recipe either (dist-only
        tools like cross-gcc on an unsupported host) → `ensure_tools` **warns**,
        tool becomes a user-provided prerequisite.
  - [ ] **`just <module> setup` → `nros setup`** (remaining): point the per-module
        setup recipes at `nros setup` so there's one install path + no duplicated
        version pins. Lower-stakes now that Method A handles the build/deploy
        flow; the justfile recipes still source-build into `build/` until flipped.
- [x] **187.7 — License gates.** `nros doctor` reads the index's `[gated.*]`
      (NVIDIA SPE, ARM FVP) and reports each: `[OK]` env→dir resolves, `[--]`
      unset (informational — not targeting that board), `[!!]` set-but-missing
      (counted). Never fetched or built — only instructed (`nros setup
      --licenses` lists the install path). Honors the redistribution boundary.

## CI & hosting (187.4 / 187.5) — where the assets live

**Decision: a separate `NEWSLabNTU/nano-ros-sdk` repo holds the prebuilt assets
+ the build matrix; the index (`nros-sdk-index.toml`) stays in nano-ros and
points at that repo's Release URLs.** It is **NOT a submodule** — nano-ros
consumes it by URL only (index → Release asset; the gate downloads). nano-ros
never builds or checks out the sdk scripts, so a gitlink would add
rebase-on-pull upkeep for no build/runtime benefit; coupling stays
one-directional + loose. Linux build runners are **Ubuntu 22.04** (Humble
baseline; a 24.04/Jazzy runner is added when Jazzy support lands). Comparison:

| | same repo (nano-ros Releases) | **separate `nano-ros-sdk` (recommended)** |
|---|---|---|
| Release/tag namespace | tool tags (`qemu-11.0-nros1`) mixed with software tags (`v1.2.3`) | clean — software releases stay software-only |
| CI budget/queue | tool build matrix runs in the main repo | isolated; doesn't compete with nano-ros CI |
| Toolchain lifecycle | coupled to nano-ros releases | independent — bump a toolchain without a nano-ros release |
| Cross-repo token | none | none for the **gate** (read-only verify of a public asset); only if main-repo CI *auto-publishes* (avoided) |
| Cost | one repo | a second repo to maintain |

**Flow (separate repo, no cross-repo write token):**
1. **`nano-ros-sdk`** holds per-tool build/repackage scripts + a
   `build-tool.yml` workflow (matrix `host × tool`): build/repackage → publish a
   Release `tag=<tool>-<version>` with `<tool>-<host>.tar.zst` (+ `.sha256`).
   Runs on `workflow_dispatch` (seed/bump a tool).
2. **nano-ros** holds `nros-sdk-index.toml` + a `sdk-index-gate.yml` workflow:
   on a PR touching the index, for each changed `[tool].version` it **downloads
   the referenced `dist.url` and verifies sha256** (read-only, public — no
   token) → required check. The gate guarantees a version reaches `main` only
   after its assets are live + hash-correct (the 187.4 guarantee, across repos).
3. The `[tool.*.source]` recipe in the index is the same one CI builds from and
   `nros setup` falls back to — so prebuilt ≡ source-built; `nano-ros-sdk` CI
   tests the source build each run.

### Maintainer checklist (the human-only / admin steps)

These need GitHub admin / network / decisions I cannot do; the agent authors
all the YAML + scripts:

- [ ] **Pick the repo** (recommend separate `nano-ros-sdk`).
- [ ] **Create `NEWSLabNTU/nano-ros-sdk`** (public; Actions enabled; workflow
      `permissions: contents: write` so the build job can publish Releases).
- [ ] **Confirm the host runner matrix** is available: `linux-x86_64` =
      `ubuntu-latest`, `linux-arm64` = `ubuntu-24.04-arm`, `macos-arm64` =
      `macos-14`, (`windows` if wanted). Only a host GitHub doesn't provide needs
      a **self-hosted runner**.
- [ ] **Per-tool redistribution/license check** before hosting: QEMU (GPL),
      cross-GCC (GPL), `zenohd` (Apache/EPL), OpenOCD (GPL) are fine; vendor SDKs
      that forbid redistribution (NVIDIA SPE, ARM FVP) are **never hosted** —
      they stay `[gated.*]`.
- [ ] **Seed the first assets:** run `build-tool.yml` (`workflow_dispatch`) for
      qemu + the cross-toolchains, so the Releases exist before the index
      references them.
- [ ] **In nano-ros: branch protection** on the index-bearing branch — require
      the `sdk-index-gate` check (so a bump can't merge before assets verify).
- [ ] *(Optional)* a **GitHub App / PAT secret** only if you later want main-repo
      CI to auto-publish to `nano-ros-sdk` (the recommended read-verify gate
      needs none).

### What the agent delivers (no admin needed)

- `nano-ros-sdk/.github/workflows/build-tool.yml` + per-tool build/repackage
  scripts (qemu, arm/riscv/xtensa GCC, zenohd, openocd).
- nano-ros `.github/workflows/sdk-index-gate.yml` + the verify script.
- The committed `nros-sdk-index.toml` skeleton (tool versions + source recipes;
  `dist` URLs/hashes filled by the gate / after seeding).

## Acceptance criteria

- [ ] A one-board first image (e.g. `nucleo_f767zi`) via `nros setup <board>` +
      `nros deploy <name>` needs **no `just`**, fetches **only** that board's
      deps (~0.5 GB, minutes — no QEMU source build), and runs.
- [ ] A host with no prebuilt `dist` builds from the `[tool.*.source]` recipe
      into the **identical** `$NROS_HOME/sdk/<tool>/<version>/` layout;
      downstream is unchanged.
- [ ] A package version bump lands on `main` **only after** its per-host assets
      are published + sha256-verified (the CI gate), proven by a test bump.
- [ ] `nros-sdk.lock` reproduces the exact toolchain set on a fresh clone.

## Notes

- **Deliberate non-goals (not gaps):** source-only distribution of nano-ros
  libraries + the user's app (the `target × arch × RTOS × RMW` binary matrix is
  a maintenance sink); the target flash floor (separate future work). See the
  comparison doc.
- **License boundary:** only redistributable tools hosted (QEMU/GCC GPL, zenohd
  Apache, OpenOCD GPL); vendor SDKs that forbid redistribution are gated.
- Open: which org/repo holds the assets (`nano-ros-sdk` suggested) + who owns
  the CI build matrix.
