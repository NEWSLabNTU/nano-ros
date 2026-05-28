# Phase 195 — `nros` bootstrap + data-driven source provisioning (cycle-free setup)

**Goal.** Make `nros setup` a genuinely `just`-free, source-checkout-free user
path by closing the two ordering gaps that remain after Phase 187/191: (A) ship
a **prebuilt `nros` binary** + a dep-free installer so a user gets `nros` without
cargo/`just`/a repo checkout, and (B) make **`[source.*]` provisioning
data-driven** (the index carries `git`/`ref`/`dest`) so `nros setup` fetches
board-scoped source from index data — never from a workspace path baked into the
binary. Keep `nros` a **generic index executor** that knows no workspace layout.

**Status.** In progress (2026-05-29). Done: the `colcon-nano-ros` → `nros-cli`
repo rename; `nros codegen` merge (195.A); **portable libc-only `nros`** (dropped
the pyo3/libpython embedding — shells out to `play_launch_parser`);
`release-binary.yml` (prebuilt per-host `nros` on `nros-v*` tags); **195.B**
data-driven `[source.*]`; **195.D** consumer switch to `nros codegen`. Remaining:
the `[tool.nros]` index entry + `install.sh` + `bootstrap.sh` (195.A); 195.C
layout decouple; 195.D crate deletion + cmake-copy dedup.

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
   download the **prebuilt `nros`** from the **`nros-cli` repo's own Releases**
   (`release-binary.yml`, `nros-<host>.tar.zst`) → drop on PATH. The binary is
   **libc-only** (no libpython — launch parsing shells out to `play_launch_parser`),
   so it runs anywhere. Pinned in the index `[tool.nros]` like any tool → the
   bump→release→sha gate guarantees availability. The contributor
   `cargo install --path` path stays and yields the identical binary. (Building
   in the CLI's own repo, not `nano-ros-sdk`, keeps the tool decoupled from the
   toolchain-asset repo it consumes.)
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
  - [x] **Merge `nros-codegen` into `nros`** (DONE — `nros codegen` subcommand,
        `cmd/codegen.rs`). Zero dep cost: `nros` already deps `cargo-nano-ros`
        (the codegen engine `nros-codegen-c` wraps), so folding the build-tool
        surface (`--args-file <json>`, `resolve-deps`, `--language c|cpp`) in as
        `nros codegen …` added nothing. Call shape kept identical to the old
        binary so the 195.D consumer switch is a binary-name change only.
        Additive — `nros-codegen-c` retained until the consumers switch + it is
        deleted (195.D). Verified `nros codegen --help` mirrors the old surface;
        `nros-cli-core` tests green.
  - [x] **Portable libc-only `nros`** (DONE). The CLI linked `play_launch_parser`,
        which embeds CPython via pyo3 `auto-initialize` → the binary required an
        exact `libpython3.X` at runtime. **abi3 can't fix an *embedding* binary**
        (pyo3 docs + verified: abi3 is extension-modules only; static embedding
        still needs the python stdlib at runtime + disables `auto-initialize`).
        Fix: `planner.rs` **shells out** to the external `play_launch_parser`
        binary (subprocess → record JSON), dropping the crate + the
        `play-launch-parser` feature. Result `ldd`: `libgcc_s` + `libc` +
        `ld-linux` only. Launch parsing delegated to the python-bearing
        `play_launch_parser` tool (`pip install play-launch-parser` / its binary;
        `NROS_PLAY_LAUNCH_PARSER` override) — the build system runs it internally
        to make the record; `--record` is not a user surface.
  - [x] **`release-binary.yml`** (DONE, in the `nros-cli` repo). Builds `nros`
        per host (linux-x86_64, linux-arm64, macos-x86_64, macos-arm64) on
        `nros-v*` tags → `nros-<host>.tar.zst` (+ `.sha256`) → Release assets;
        host names match `SdkIndex::host_key()`. The PyPI wheel workflow
        (`release.yml`) is guarded off for `nros-v*` tags.
  - [x] **`[tool.nros]` index entry** (DONE). `version = "0.2.0"` + a `source`
        recipe (`git submodule update --init --recursive` + `cargo install --path
        packages/nros-cli --root {prefix}` from the nros-cli repo). Per-host
        `dist` (pointing at the **nros-cli** repo Releases, not nano-ros-sdk) is
        added after the first `nros-v*` release is cut + gate-verified — until
        then it falls back to the source recipe (the sanctioned "leave dist out
        until seeded" pattern). Verified `nros setup --list` shows `nros 0.2.0`.
  - [x] **`install.sh`** (DONE, in the nros-cli repo). `curl … | sh`: detects
        host (matches `SdkIndex::host_key`), downloads `nros-<host>.tar.zst` +
        `.sha256` from the nros-cli Releases (`nros-v<version>`), verifies, installs
        to `$NROS_HOME/bin`, prints PATH guidance. No cargo/just/checkout.
        (Activates once the first `nros-v*` release exists.)
  - [x] **`scripts/bootstrap.sh nros`** (DONE). A `nros` verb — the just-free
        prebuilt route — runs the in-tree `packages/codegen/install.sh` (or
        fetches it over the network) to install the prebuilt `nros`, *before*
        `ensure_just` (no rustup/just/cargo). The existing `""`/`base`/`all`
        verbs keep the contributor rustup+just+`just setup` source route. (The
        download activates once the first `nros-v*` release exists.)
- [x] **195.B — Data-driven `[source.*]` provisioning (Gap B). DONE
      (2026-05-29).** Mechanism + data + the `tools/setup.sh` consumer rewiring
      all landed; the index is the SSOT for source refs.
  - [x] Extend `SourcePackage` (`orchestration/sdk_index.rs`): added `git`,
        `ref`, `dest` (workspace-relative), optional `submodule` (the
        `.gitmodules` path; `git`/`ref` still record the pin = SSOT, so the two
        can't drift). `git`+`ref`+`dest` describe the source; `submodule` is a
        *mode hint* (submodule-update vs fresh clone), **not** mutually
        exclusive. `deny_unknown_fields` kept; `provision()` picks the mode;
        `validate` checks coherence (clone needs `ref`+`dest`; submodule needs
        `dest`). Unit-tested.
  - [x] `nros setup` / `ensure_tools`: `sdk_store::provision_source` does
        clone-`git`@`ref`-into-`dest` (full clone — `ref` may be a sha) or
        `git submodule update --init <submodule>`, **idempotent** (a populated
        `dest` is left untouched). Wired into `nros setup <board>` (per-source
        disposition line) + the lazy `ensure_tools` (build/deploy auto-setup).
        `dest` is always index data, never a baked path. Unit-tested + verified
        via `nros setup qemu-arm-freertos --dry-run` (plans freertos-kernel +
        lwip provisioning).
  - [x] Make the index the **single source of truth** — DONE. Added the
        primitive **`nros setup --source <name>`** (repeatable; index-driven;
        mirrors 187.6's `--tool`) and rewired the consumer: `tools/setup.sh`'s
        fetch loop reads the index `[source.*]` (`read_index_source_paths`),
        and for a submodule path that matches a source's `submodule`/`dest` it
        delegates to `nros setup --source <name>` (`resolve_nros_bin` finds an
        `nros` on PATH or the cargo-built one in the codegen target dir).
        **No 195.A dependency** — the unblock was realising `just setup` is the
        *contributor* path (always has cargo + checkout), and for a
        **submodule-mode** source `nros setup --source` runs *exactly*
        `git submodule update <path>`, so the pre-rustup fallback to plain
        `git submodule update` is an **equivalence, not a fragile guess**. The
        index ref/url is the SSOT; `submodule-deps.toml` stays as the
        platform→path map (matched by path → no drift on refs). 195.A's
        prebuilt `nros` only matters for the *no-checkout end-user*, who calls
        `nros setup <board>` directly (sources via B.2) and never touches
        `tools/setup.sh`.
  - [x] Fill the real `git`/`ref`/`dest`(+`submodule`) for the four current
        `[source.*]` entries (`nros-sdk-index.toml`) from the submodule pins +
        recorded gitlink SHAs.
- [ ] **195.C — Decouple the CLI's runtime nano-ros layout knowledge.**
      **Design + data DONE; the CLI reader is NOT implemented** (corrected
      2026-05-29 — the previous "done" claim was false: no `BoardCatalog` /
      `board_descriptor.rs` exists in any `nros-cli` ref, and `generate.rs`'s
      `profile()` is still a baked `match board { … }` + `PlatformProfile` /
      `BoardEntry` consts + per-`PlatformKind` `.cargo/config.toml`/dep bodies
      with the `nros-board-*` / `packages/boards/…` / `third-party/<kernel>/`
      literals, as of nros-cli `da75c37`).
  - [x] **Descriptors authored** (nano-ros side, landed). `packages/boards/*/nros-board.toml`
        (+ crate-less `packages/boards/posix/` etc.) — a `[[board]]` array per
        file with the full schema: `names` (+ `target_contains` to disambiguate
        same-named boards like threadx riscv64 vs linux), `platform`, `toolchain`,
        `platform_feature`, `local_aliases`, `link_kind`, `entry_kind`,
        `net_stack`, `chip`, `board_crate`, `board_features`, a **verbatim
        `cargo_config`** body using `${workspace}` placeholders (carries the
        kernel-port / config paths as data), `libc`/`linker`/`runner`/`rustflags`,
        and a `[board.entry]` block (`crate_name`, `signature`, `comment`,
        `crate_root_extra`, `closure_extra`).
  - [ ] **CLI reader (the remaining work).** Implement `orchestration/board_descriptor.rs`:
        serde structs for the schema + `BoardCatalog::load(workspace)` (glob
        `packages/boards/*/nros-board.toml`) + `resolve(board, target)` (name +
        `target_contains` match → `PlatformProfile` + `BoardEntry`, `${workspace}`
        substituted). Migrate `generate.rs`: `profile()` → `catalog.resolve()`;
        the per-`PlatformKind` `.cargo/config.toml` emission → the descriptor's
        `cargo_config`; the entry block → `[board.entry]`; the board-crate/extra
        deps → descriptor + convention. Then delete the baked `match`.
  - [ ] **Byte-identical safety net.** The 17 `orchestration_generate` snapshot
        tests must stay unchanged; capture goldens for any board not yet
        snapshotted before migrating. Runs in standalone nros-cli CI (goldens +
        a fixture descriptor set in the repo test tree).
- [ ] **195.D — Retire the `packages/codegen` submodule from nano-ros (end state).**
      Once the merged `nros` is a host binary (195.A) and the CLI is
      layout-decoupled (195.C), nano-ros no longer needs the CLI *source* in-tree.
      Blockers to clear first:
  - [x] **In-tree consumers switched to `nros codegen`** (DONE, verified). Both
        codegen-tool callers now build + invoke the `nros` binary:
        - `scripts/build/cargo.sh` (`nros_cargo_*codegen_c*` → `-p nros-cli --bin
          nros`, path `…/nros`), and the per-recipe path literals in
          `just/{nuttx,freertos,threadx-linux,threadx-riscv64,zephyr}.just` +
          `scripts/zephyr/check-copy-out.sh`.
        - **Both** `NanoRosGenerateInterfaces.cmake` copies (root `cmake/` for
          POSIX; the submodule `nros-codegen-c/cmake/` copy for cross-compile) —
          `COMMAND … codegen …` + `find_program(nros)`; `NanoRosBootstrapCodegen.cmake`
          + root `CMakeLists.txt` POSIX Corrosion target → `nros`
          (`nros-cli/CMakeLists.txt`).
        - The direct-invoker scripts `scripts/nuttx/gen-interfaces.py` +
          `gen-cpp-ffi-crates.py` (insert `codegen`; default path `…/nros`).
        Verified: `just nuttx build-fixtures` green (6 C + 6 C++ FFI, `nros`
        built, `nros-codegen` absent) **and** native-posix C talker codegen green
        (Corrosion `nros` target → `nros codegen`).
        **Caveat surfaced:** there are **two drifting copies** of
        `NanoRosGenerateInterfaces.cmake` (+ its `*.in` templates) — root `cmake/`
        and submodule `nros-codegen-c/cmake/` — included by the POSIX root vs the
        `freertos`/`threadx`/`nuttx` platform modules respectively. **Deleting the
        `nros-codegen-c` crate is blocked on deduping these** (relocate the
        submodule copy → `nros-cli/cmake/`, repoint the 3 platform includes); the
        crate is kept (unused as a tool) until then.
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
- [x] **195.E — Refresh the `nros-cli` repo's README + CLI help text.** DONE
      (`da75c37`). `README.md` rewritten around the `nros` CLI (was the
      colcon-cargo-ros2 / PyPI doc): prebuilt install (`curl|sh install.sh`), the
      **unified `nros setup <board>`** flow (board-scoped toolchains **+** source
      provisioning from the SDK index — verified `qemu-arm-freertos` pulls
      arm-gcc + qemu + FreeRTOS-Kernel + lwip), user-import vs contributor build
      paths, the command list, and the "generic tool, no baked nano-ros layout"
      principle. Top-level `--help` long_about gained a quick-start. (Also:
      the failing colcon/python CI workflows were removed + ci.yaml rewritten
      Rust-only — green; see the workflow commits.)

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
