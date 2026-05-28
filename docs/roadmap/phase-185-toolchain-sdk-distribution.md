# Phase 185 — Toolchain & SDK distribution (`nros setup`)

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
one-board user, and it needs `just`. Phase 185 closes that gap.

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

- [ ] **185.1 — Package index format + loader.** Define + parse
      `nros-sdk-index.toml` (`[tool]`/`[source]`/`[gated]`, per-host `dist`,
      `[tool.*.source]` recipe, version + sha256). **Files:**
      `packages/codegen/.../orchestration/sdk_index.rs` (or a new `nros-sdk`
      crate), the committed `nros-sdk-index.toml`.
- [ ] **185.2 — `nros setup` CLI + board resolution.** `nros setup <board>` /
      `--target` / `--list` / `--licenses`; resolve board→package set via
      `profile()`/board crates. **Files:** `nros-cli-core/src/cmd/setup.rs`,
      `cmd/mod.rs`.
- [ ] **185.3 — Fetch / verify / cache / source-fallback / lockfile.** Download
      host-matched `dist`, sha256-verify, unpack to `$NROS_HOME/sdk/<tool>/<ver>`;
      no `dist` ⇒ build from `[tool.*.source]` @ ref into the same prefix
      (identical layout, `.nros-provenance`); shared store; write/read
      `nros-sdk.lock`.
- [ ] **185.4 — Versioning + CI bump→release gate.** Per-host build matrix →
      draft Release (`tag=<tool>-<version>`, `asset=<tool>-<host>.tar.zst`) →
      sha256 back-commit → required check → publish on merge. The source recipe
      is CI-tested each run so prebuilt ≡ source-built. **Files:**
      `.github/workflows/sdk-release.yml`.
- [ ] **185.5 — `nano-ros-sdk` hosting repo + prebuilt builders.** A repo
      holding the Release assets + per-tool build/repackage scripts for QEMU,
      cross-GCC (arm/riscv/xtensa), `zenohd`, OpenOCD across the host matrix.
- [ ] **185.6 — Unify with `just setup` + auto-install-on-build.** `just
      <module> setup` becomes a thin caller of the same index (one truth, no
      drift); `nros build`/`deploy` trigger a missing `nros setup <board>` (the
      PlatformIO lazy-install ergonomic).
- [ ] **185.7 — License gates.** NVIDIA SPE / ARM FVP: never fetched or built —
      instruct + expected env var; `nros doctor` presence check (already exists
      from Phase 172).

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
