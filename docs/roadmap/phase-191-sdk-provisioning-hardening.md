# Phase 191 — SDK provisioning hardening (`nros setup` SSOT cleanup)

**Goal.** Remove the SSOT violations + antipatterns a code audit found in the
Phase 187 toolchain/SDK surface — chiefly the board→package keyword heuristic and
the toolchain version pins duplicated between the index and the build scripts.

**Status.** Complete — 191.1–191.5 done (codegen `16bb76b`).

**Priority.** P2 — correctness/maintainability of a landed feature; the heuristic
already produced one bug (esp32 mis-resolved as Xtensa, fixed in 187 cleanup).

**Depends on.** Phase 187 (archived). Surfaces: `nros-cli-core/src/cmd/setup.rs`,
`orchestration/sdk_index.rs`, `nros-sdk-index.toml`, `ci/nano-ros-sdk/scripts/`.

## Overview

Phase 187 shipped `nros setup` + the SDK index + the nano-ros-sdk prebuilt repo.
An audit found two SSOT violations that cause real drift (one already bit) plus
several lower-severity antipatterns. This phase fixes the two SSOT issues and
records the rest.

## Architecture

- **End-user constraint:** a user running `nros setup <board>` has only the
  `nros` binary + `nros-sdk-index.toml` + the prebuilt Releases — **no source
  tree**. So board→toolchain knowledge must ship *with the index*, not be read
  from `packages/boards/` (the board crates the maintainer-side CLI can see).
- **Cross-repo constraint:** the build/repackage scripts live in the separate
  `nano-ros-sdk` repo and cannot read nano-ros's index. The index is the
  authoritative *record* of each tool's exact upstream pin; the scripts must
  *consume* that pin (workflow input), not hardcode it.

## Work items

- [x] **191.1 — Board→package resolution is data, not a keyword heuristic
      (audit #1, HIGH).** DONE. `resolve_packages(index, board)` reads a
      `[board.*]` table (arch/platform/packages); 14 boards mapped; unknown board
      → error listing known boards; esp32 = riscv32 / no host-tool. Dropped the
      dead `--target` flag + `ensure_tools`'s unused `target` param. `resolve_packages` matches board-name/target substrings
      (`b.contains("esp32")`, `"stm32"`, `"mps2"`, `"freertos"`, …) with tool
      names as bare `&'static str` — re-encoding board facts that belong to the
      board, and silently wrong for any board the match doesn't anticipate (the
      esp32→Xtensa bug). Replace with a `[board.<name>]` table in the index
      (`arch`, `platform`, optional `sim`) that ships with the index; resolve
      derives tools from the declared arch/platform via a small stable mapping
      (arch families don't churn). Unknown board → clear error, not a silent
      wrong guess. **Files:** `sdk_index.rs`, `cmd/setup.rs`, `nros-sdk-index.toml`.
- [x] **191.2 — Toolchain upstream pin is recorded in the index (audit #2,
      HIGH).** DONE. `[tool.*].upstream` records the exact rev (13.2.rel1,
      14.2.0-3, the qemu fork branch, …); `build-tool.yml` takes an `upstream`
      input + the 5 build scripts read it as `$3` (no hardcoded/hand-derived
      pins). Synced to the nano-ros-sdk repo. The exact upstream rev of a repackaged tool lives only in the
      build script (`build-riscv-none-elf-gcc.sh` hardcodes xPack `14.2.0-3`;
      `build-arm` hand-derives `13.2.rel1` from the version string). The index —
      the supposed SSOT — is lossy. Add an `upstream` field to `[tool.*]`
      recording the exact rev; the build scripts take it as an explicit argument
      (via the `build-tool.yml` `upstream` input) instead of hardcoding/deriving.
      **Files:** `sdk_index.rs`, `nros-sdk-index.toml`,
      `ci/nano-ros-sdk/scripts/build-*.sh`, `ci/nano-ros-sdk/.github/workflows/build-tool.yml`.
- [x] **191.3 — qemu `configure` flags duplicated (audit #3, MED).** DONE.
      Can't share at runtime (the build script is in the separate nano-ros-sdk
      repo), so `scripts/sdk/check-qemu-configure.sh` diffs the flag sets of the
      index `[tool.qemu.source].configure` and the `ci/nano-ros-sdk/` seed
      `build-qemu.sh` (both present in this repo) and fails on drift; wired into
      `sdk-index-gate` (paths extended to the build script).
- [x] **191.4 — Cross-check resolver names against the index (audit #4, MED).**
      DONE. `SdkIndex::validate()` (called by `load()`) errors if any
      `[board.*].packages` name isn't a defined `[tool]/[source]/[gated]` entry —
      no more silent skip. (`resolve_packages` now emits only `[board.*].packages`
      names, so this covers the whole resolver vocabulary.)
- [x] **191.5 — Lower-severity cleanups (audit #5–#9).** Done: `LOCK_FILE`
      const for the 3 `nros-sdk.lock` literals; dropped `brew install … || true`
      (fail loud); documented the cwd lock location (the `LOCK_FILE` doc comment).
      **Won't-do (with rationale):** #5 `activate_store_path`'s `set_var("PATH")`
      stays — Method A activates the toolchain process-wide for the children
      `nros build`/`deploy` spawn (incl. the orchestration build deep in the
      call tree); per-`Command` env would thread dirs through every spawn site
      for no real safety gain (single-threaded CLI, set before any child). #7 the
      host→arch maps in the build scripts are vendor-specific naming
      (ARM `aarch64` vs xPack `linux-arm64` vs `darwin-arm64`) — not dedupable.
      The `nros-sdk-index.toml` filename stays a clap `default_value` literal (the
      derive attribute can't take a const).

## Acceptance criteria

- [x] `resolve_packages` contains no board-name substring matching; adding a board
      is an index `[board.*]` entry, no Rust edit. A bogus board errors clearly.
- [x] Every repackaged tool's exact upstream rev is in `nros-sdk-index.toml`; no
      build script hardcodes or hand-derives a version.
- [x] `cargo test -p nros-cli-core` green (124); `nros setup <board> --dry-run`
      resolves from the index for known boards, errors on unknown.

## Notes

- The audit also confirmed **no issues** in `sdk_index.rs`/`sdk_store.rs` schemas
  (versioned-prefix SSOT, provenance, sha256 verify), no hardcoded repo-relative
  paths in Rust, and correct just-recipe-local `build/` paths.
- A later step can **codegen the index `[board.*]` table from the board crates'
  `profile()`/metadata** so the board crate stays the ultimate SSOT and the index
  is a generated artifact — out of scope here (191.1 hand-authors the table).
