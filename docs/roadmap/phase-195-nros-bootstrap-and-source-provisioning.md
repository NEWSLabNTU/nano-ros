# Phase 195 — `nros` bootstrap + data-driven source provisioning (cycle-free setup)

**Goal.** Make `nros setup` a genuinely `just`-free, source-checkout-free user
path by closing the two ordering gaps that remain after Phase 187/191: (A) ship
a **prebuilt `nros` binary** + a dep-free installer so a user gets `nros` without
cargo/`just`/a repo checkout, and (B) make **`[source.*]` provisioning
data-driven** (the index carries `git`/`ref`/`dest`) so `nros setup` fetches
board-scoped source from index data — never from a workspace path baked into the
binary. Keep `nros` a **generic index executor** that knows no workspace layout.

**Status.** Not started — design approved 2026-05-29 (review of the Phase 187
setup workflow surfaced these as the unimplemented half of "replace `just
setup`").

**Priority.** P2 — first-image onboarding UX; completes the Phase 187 user-path
promise. No MVP capability depends on it.

**Depends on.** Phase 187 (`nros setup`, the index format, the
`nano-ros-sdk` Releases host, the bump→release→sha gate), Phase 191 (`[board.*]`
SSOT, `SdkIndex::validate`), Phase 172 (the `nros` CLI). **Design:**
`docs/design/nros-setup-toolchain-management.md`.

## Overview

There is **no true cyclic dependency**: `nros` is a pure cargo build
(`cargo install --path packages/codegen/packages/nros-cli`) whose only
non-registry deps are small Rust crates (`play_launch_parser`,
`ros-launch-manifest`) — **zero SDK toolchains**. The dependency graph is a
one-way DAG:

```
nros (cargo, no SDK)  →  nros setup (index-driven)  →  prebuilt tools + source
```

What *looks* cyclic is **bootstrap ordering**: running `nros setup` needs the
`nros` binary, and today the only way to obtain it is `cargo install` from a
checkout (needs Rust + the repo). `scripts/bootstrap.sh` still installs
rustup → `just` → `just setup` (all source). So the user path is not yet
`just`-free. Two concrete gaps:

- **Gap A — no prebuilt `nros`.** No host binary, no installer. The
  chicken-and-egg is purely "get `nros` before `nros setup`".
- **Gap B — `[source.*]` is not data-driven.** Index `[source.*]` entries
  (`freertos-kernel`, `lwip`, `threadx`, `zenoh-pico`) carry **only `version`**;
  `nros setup` does nothing with them. They come from git submodules pulled by
  `just`/clone. Moving that checkout into `nros` with hardcoded `third-party/`
  paths would make `nros` **workspace-structure-aware** and unfixable without
  reshipping the binary — the anti-goal.

## Architecture

**Governing principle: `nros` is a generic index executor. ALL
workspace/layout knowledge lives in the committed `nros-sdk-index.toml` (data —
versioned, CI-gated).** A path/URL/source-ref fix is a data edit (index bump +
gate), never an `nros` rebuild. The binary is rev'd only for CLI *logic*. This
is what keeps `nros` workspace-agnostic and the layout fixable.

Three layers, each strictly downstream of the previous (the cycle-break):

1. **Bootstrap installer** (tiny, dep-free, rustup model): detect host×arch →
   download the **prebuilt `nros`** from `nano-ros-sdk` Releases → drop on PATH.
   `nros` is just another host artifact in `build-tool.yml`'s matrix
   (`nros-<host>.tar.zst`), pinned in the index like any tool → the same
   bump→release→sha gate guarantees its availability (no dangling version). The
   contributor `cargo install --path` path stays and yields the identical
   binary.
2. **`nros setup <board>`** reads the committed index and installs board-scoped:
   `[tool.*]` → prebuilt host tools (done, 187); `[source.*]` → fetch from the
   new `git`/`ref`/`dest` fields into a store/vendor dir. The destination is
   *index data*, never a hardcoded `third-party/` path.
3. **`nros build`/`deploy`** — Method A PATH injection (done, 187.6).

Layer 1 has zero dependency on layers 2–3's outputs → no cycle.

## Work items

- [ ] **195.A — Prebuilt `nros` + bootstrap installer (Gap A).**
      **Build the releases in the CLI's own repo** — `nros-cli` already lives in
      the `colcon-nano-ros` submodule (`github.com/NEWSLabNTU/colcon-nano-ros`),
      a self-contained Rust workspace with **zero path deps into the nano-ros
      superproject** (verified: all `path =` deps stay within the submodule). The
      plan: **rename `colcon-nano-ros` → `nros-cli`** and have *that* repo build
      + publish the host binaries on its own Releases (cleaner separation than
      adding `nros` to `nano-ros-sdk`'s tool matrix; the CLI versions track the
      CLI repo, not the toolchain repo).
  - [ ] **Merge `nros-codegen` into `nros` first** (recommended, zero dep cost):
        `nros` already deps `cargo-nano-ros` (the codegen engine `nros-codegen-c`
        wraps) and already has `generate`/`generate-rust`/`generate-c`
        subcommands — its dep tree is a **superset** of `nros-codegen`'s, so
        folding the build-tool-shaped surface (`--args-file <json>`,
        `resolve-deps`, `generate-c/cpp`) in as `nros codegen …` adds nothing.
        Keep the JSON `--args-file` call shape stable so the 92 in-tree consumers
        (195.D) change only the binary name (`nros-codegen …` → `nros codegen …`).
        Payoff: **one** host artifact to build/ship/install, one fewer thing to
        switch. The only reason to keep it separate (a leaner build-time tool)
        evaporates once it's prebuilt.
  - [ ] In the (renamed) `nros-cli` repo: a release workflow building `nros`
        (one binary, post-merge) per host (linux-x86_64, linux-arm64,
        macos-arm64), `cargo build --release` (the `packages/` build-infra
        workspace needs no ROS), packaged `nros-<host>.tar.zst` (+ `.sha256`).
  - [ ] `nros-sdk-index.toml`: a `[tool.nros]` entry (single binary post-merge) —
        `version` + per-host `dist` (pointing at the `nros-cli` repo's Releases)
        + a `source` recipe (`cargo install --path`) fallback. Subject to the
        existing `sdk-index-gate`.
  - [ ] `install.sh` (`curl … | sh`, rustup model): detect host, read the pinned
        version, download + verify + install `nros` to `$NROS_HOME/bin` (or
        `~/.local/bin`), print PATH guidance. No cargo / no `just` / no checkout.
  - [ ] `scripts/bootstrap.sh`: offer the prebuilt path alongside the existing
        rustup+just source path; default to prebuilt when a `dist` for the host
        exists.
- [ ] **195.B — Data-driven `[source.*]` provisioning (Gap B).**
  - [ ] Extend `SourcePackage` (`orchestration/sdk_index.rs`): add `git`,
        `ref`, `dest` (workspace-relative destination), optional `submodule`
        (the `.gitmodules` path when the canonical source is a submodule).
        Keep `deny_unknown_fields`; bump `SdkIndex::validate` to check
        `[board.*]` source refs resolve.
  - [ ] `nros setup` / `ensure_tools`: provision a board's `[source.*]` set from
        the index data — clone `git`@`ref` (or `git submodule update` the named
        path) into `dest`, idempotent (skip if present at the right ref). Never
        a hardcoded path; `dest` comes from the index.
  - [ ] Make the index the **single source of truth** for source refs:
        `just <module> setup` becomes a thin caller reading `[source.*]` (mirrors
        what 187.6 did for `qemu`/`zenohd`), so submodule `.gitmodules` and the
        index can't drift.
  - [ ] Fill the real `git`/`ref`/`dest` for the four current `[source.*]`
        entries from the existing submodule pins.
- [ ] **195.C — Decouple the CLI's runtime nano-ros layout knowledge.**
      *Cargo-dep-free ≠ nano-ros-knowledge-free.* `nros-cli-core` builds standalone,
      but at **runtime** it bakes the nano-ros workspace *layout*: `generate.rs`
      alone has ~20 `workspace.join("packages/...")` / `third-party/...` literals
      and **8 hardcoded board-crate names** (`nros-board-stm32f4`,
      `nros-board-threadx-qemu-riscv64`, …) plus board-specific kernel-port paths
      (`third-party/threadx/kernel/ports/risc-v64/...`, `third-party/nuttx/libc`).
      For a binary shipped from a *separate* repo, this layout is the workspace's
      data, not the tool's code.
  - [ ] Move per-board layout (board-crate path, kernel-port subpaths, the
        source set) into **board descriptors read from the workspace** (extend
        the `[board.*]` index table / the board crates' `profile()`), so the CLI
        resolves paths from data, not `match board { … }` literals.
  - [ ] Acceptance: grep `nros-cli-core/src` for `nros-board-` /
        `third-party/<kernel>/` literals → none remain; a new board needs only a
        descriptor + crate, no CLI edit (mirrors Phase 194's de-hardcode for
        NuttX, one level up).
- [ ] **195.D — Retire the `packages/codegen` submodule from nano-ros (end state).**
      Once the merged `nros` is a host binary (195.A) and the CLI is
      layout-decoupled (195.C), nano-ros no longer needs the CLI *source* in-tree.
      Blockers to clear first:
  - [ ] **92 in-tree consumers** in `justfile`/`just/*.just`/cmake reference the
        *built* path `packages/codegen/packages/target/.../nros-codegen` /
        `_NANO_ROS_CODEGEN_TOOL` / `cargo install --path packages/codegen/...` —
        switch every one to the host binary on PATH (`nros-codegen …` →
        `nros codegen …`; Method A already injects PATH).
  - [ ] **Non-CLI tenants** of the submodule (`colcon-cargo-ros2`,
        `cargo-nano-ros`, `rosidl-{parser,codegen,bindgen}`, the `user-libs`
        rclrs/rosidl-runtime-rs) — confirm nano-ros doesn't build them in-tree, or
        relocate. (These stay in the renamed `nros-cli` repo; nano-ros just stops
        gitlinking it.)
  - [ ] **Codegen runtime data** — orchestration templates are `include_str!`'d
        (already embedded); confirm bundled interfaces (`packages/codegen/interfaces/`)
        are embedded or relocated into nano-ros proper.
  - Then: drop the gitlink. **Installing the host binary alone is NOT sufficient**
        — the submodule is today a *build* dependency of nano-ros (the 92 hooks),
        not just the CLI's home.

## Acceptance criteria

- [ ] A fresh machine with **no Rust, no `just`, no checkout** can
      `curl …/install.sh | sh` → get `nros` → `nros setup <board>` →
      `nros deploy <name>` and run a first image. No source build of `nros`.
- [ ] `nros setup <board>` provisions that board's `[source.*]` (e.g. FreeRTOS
      kernel) from **index data** into the index-declared `dest`; the same
      `nros` binary works for a different board's different source set with no
      rebuild.
- [ ] Editing a source `ref` (or a tool URL) in `nros-sdk-index.toml` + passing
      the `sdk-index-gate` is sufficient to fix a provisioning issue — **no
      `nros` rebuild/respin** — proven by a test bump.
- [ ] The prebuilt `nros` version is in `main` **only after** its per-host
      assets are published + sha-verified (the 187.4 gate, applied to `nros`).
- [ ] `nros` source has **no hardcoded `third-party/` provisioning path** —
      grep-clean; all destinations come from the index.

## Notes

- **Not a cycle, an ordering.** Documented above so future readers don't
  re-litigate: `nros` builds with cargo alone; the gaps are bootstrap delivery
  (A) + data-driven source (B), not a dependency loop.
- **One SSOT for source refs.** Prefer the index over `.gitmodules` as
  canonical; submodules remain for the contributor checkout but are kept in sync
  *from* the index, not the reverse. Honors the existing "don't modify
  vendored/generated" rule — provisioning writes to `dest`, never edits vendored
  trees.
- **License boundary unchanged** — only redistributable artifacts hosted; `nros`
  itself (the repo's own binary) is trivially redistributable.
- **`nros-cli` is already Cargo-standalone** — verified zero `path =` deps from
  `nros-cli`/`nros-cli-core` into the nano-ros superproject; all stay within the
  `colcon-nano-ros` submodule. So building releases in that (renamed) repo is
  viable today. The remaining coupling is runtime *layout* knowledge (195.C), not
  build deps.
- **Submodule removal is the end state, gated on 195.A+C** and the 92-consumer
  switch (195.D) — not a free consequence of shipping the host binary.
- **NuttX caveat:** the NuttX export stays source-built + self-provisioned per
  Phase 194 (it is target-specific, not a host tool) — 195.B's `[source.*]`
  data-drive is the generic mechanism that a NuttX board's kernel source slots
  into, but the `make export` step remains the board crate's job.
