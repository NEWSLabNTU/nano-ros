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

- [x] **195.A — Prebuilt `nros` + bootstrap installer (Gap A). DONE.**
      `colcon-nano-ros` was renamed → **`nros-cli`** (`github.com/NEWSLabNTU/nros-cli`),
      a self-contained Rust workspace with zero path deps into the nano-ros
      superproject. That repo builds + publishes the host binaries on its own
      Releases (`release-binary.yml`, tag `nros-v*`, 4-host `nros-<host>.tar.zst`
      + sha256); `scripts/install-nros.sh` / `curl install.sh | sh` fetch them.
      Latest release: `nros-v0.3.0` (195.D.4).
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
- [x] **195.C — Decouple the CLI's runtime nano-ros layout knowledge.** DONE
      (2026-05-29). The reader lived on an unmerged nros-cli `195.c` branch — it
      was cherry-picked onto main (`458154c`) and verified. `generate.rs`'s
      `profile()` is now `BoardCatalog::load(workspace) + resolve(board, target)`
      — **zero** `match board { … }`, **zero** `workspace.join("packages/boards"|
      "third-party")` literals; the binary is layout-agnostic. Verified: 132
      nros-cli-core lib tests + 18 `orchestration_generate` snapshots pass
      (codegen **byte-identical**), clippy `-D warnings` + nightly-fmt clean.
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
  - [x] **CLI reader. DONE (`458154c`).** `orchestration/board_descriptor.rs` +
        `BoardCatalog::load(workspace)` (glob `packages/boards/*/nros-board.toml`) +
        `resolve(board, target)` landed; `generate.rs` migrated `profile()` →
        `catalog.resolve()` (the per-`PlatformKind` `.cargo/config.toml` emission →
        the descriptor's `cargo_config`, entry block → `[board.entry]`), baked
        `match` deleted (+410/-845 in `generate.rs`). Shipped in `nros-v0.3.0`.
  - [x] **Byte-identical safety net. DONE.** The `orchestration_generate` snapshot
        tests stay green in standalone nros-cli CI ("Rust Tests" job, run
        `26621672306` on `97ff3fc`).
- [x] **195.D — Retire the `packages/codegen` submodule from nano-ros (end state). DONE.**
      The merged `nros` is a host binary (195.A) and the CLI is layout-decoupled
      (195.C), so nano-ros no longer needs the CLI *source* in-tree. Gitlink
      removed + every consumer rewired to the installed `nros` (`e2f01a945`).
      Blockers cleared:
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
  - [x] **`nros-codegen-c` crate deleted** (DONE, nros-cli `1e7e879`). The
        feared two-copy dedup was a non-issue: the `freertos`/`threadx`/`nuttx`
        platform modules already `include()` the **root** `cmake/NanoRosGenerate‐
        Interfaces.cmake` (`${CMAKE_CURRENT_LIST_DIR}/../…`), not the submodule
        copy — so `nros-codegen-c/cmake/` was dead, as was its Rust bin (consumers
        use `nros codegen`) and its Corrosion `CMakeLists` (root uses
        `nros-cli`). Removed the crate + its workspace-member entry; codegen
        workspace builds clean. Root cmake doc + `NanoRosBootstrapCodegen.cmake`
        comments de-stale'd (single canonical module).
  - [x] **Drop the gitlink — DONE (`e2f01a945`, 2026-05-29).** `git rm packages/codegen`
        + `.gitmodules` entry dropped; every consumer (CI, cmake, cargo.sh,
        config, tests, setup.{bash,fish}, justfile, scripts) rewired to the
        installed `nros`; verified `just nuttx build-fixtures` green with the
        submodule physically removed. The blocker analysis below is kept as
        history. A first attempt
        (rewire the build to a prebuilt/installed `nros`, install `nros-v0.2.0`)
        appeared to fail on `action_msgs → builtin_interfaces` not resolving — but
        **that was a self-inflicted stale-cache artifact**, not a real limitation:
        a broken `~/.cargo/bin/nros` (a pre-`codegen`-merge cargo-install) was on
        PATH, failed `resolve-deps`, and left a bad cmake cache. With it removed,
        the prebuilt's `nros codegen resolve-deps action_msgs` correctly lists
        `builtin_interfaces` + `unique_identifier_msgs`. **The codegen engine is
        NOT a blocker** (and `cargo-nano-ros` is byte-identical da75c37→main). The
        genuine blockers to a clean drop:
    1. **Bundled-interfaces locator.** `cargo-nano-ros::bundled_interfaces_dir()`
       finds the no-ROS fallback `.msg` defs by walking up from the binary to
       `packages/codegen/interfaces` — an installed prebuilt can't. Moot when a
       ROS env is sourced (`AMENT_PREFIX_PATH` provides them; the in-tree
       `interfaces/` dir is in fact absent in this checkout), but a **no-ROS host**
       would fail. Fix: embed the defs in the binary (`include_dir!`), ship them
       in the release + locate via the install layout, or an `NROS_INTERFACES_DIR`
       env.
    2. **Build-flow.** 28 recipe/script sites + the POSIX root `CMakeLists`
       Corrosion build `nros` from the submodule. Rewire them to a resolved
       installed `nros` (cargo.sh resolver `$NROS`/PATH/`$NROS_HOME/bin` —
       prototyped + reverted) and wire `just setup`/CI to install it (`install.sh`)
       so the build finds it.
    3. **Co-development** (accepted trade-off): no in-tree `nros` source ⇒
       changing `nros` = edit the `nros-cli` repo → release / `cargo install --git`.
       Re-cut a release tracking main + pin it in `[tool.nros]` before the drop.
  - **Expanded work items (195.D.1–.5):**
  - [x] **195.D.1 — Interface-resolution: confirmed a non-blocker.** Verified
        nano-ros codegen **requires a sourced ROS** (`AMENT_PREFIX_PATH`) for the
        standard defs (`builtin_interfaces`/`std_msgs`/`action_msgs` from
        `/opt/ros/…`): without AMENT *neither* the prebuilt nor the submodule-built
        `nros` resolves them. The `packages/codegen/interfaces` bundled fallback
        dir does not exist (vestigial). So prebuilt+AMENT ≡ submodule+AMENT — no
        regression, no fix needed for the drop. *(Optional later hardening: embed
        a minimal interface set in the binary for no-ROS hosts — an enhancement,
        not a drop blocker.)*
  - [x] **195.D.2 — Rewire the codegen-tool source to an installed `nros`.**
        **DONE + verified (`96b835b53`).** `nros_cargo_codegen_c_bin` now delegates
        to `nros_cli_bin` (resolves `$NROS_CLI` → PATH → `${NROS_HOME:-~/.nros}/bin`)
        and returns an absolute path; `nros_cargo_ensure_codegen_c` is a
        presence-check (no submodule cargo build). All 13 per-platform recipe
        assignments (nuttx ×3, zephyr ×4, native ×3, freertos, threadx-linux,
        threadx-riscv64) drop their `$(pwd)/`/`$root/`/`$nros_root/`/`${repo_root}/`
        prefix and use the bare resolver. **Verification** (after a full
        environment recovery — see note below): `just nuttx build-fixtures` builds
        all 6 Rust examples + the logging-smoke fixture with the installed nros;
        the installed `~/.cargo/bin/nros` and the in-tree
        `packages/codegen/packages/target/release/nros` are **byte-identical**
        (same sha256, both `dd18511`), so the source swap is a provable functional
        no-op. The earlier "inconclusive" mystery (the per-package generate loop
        not emitting `builtin_interfaces`) turned out to be a **pre-existing,
        tool-independent** gap in the cmake macro, NOT a rewire defect: the action
        auto-recursion (`NanoRosGenerateInterfaces.cmake` ~276-284) generates
        `unique_identifier_msgs` + `action_msgs` but **not** `builtin_interfaces`,
        even though `action_msgs/GoalInfo` needs `builtin_interfaces/Time` — so
        `action_msgs_msg_goal_info.h` can't find its header. Identical bits ⇒ fails
        the same way with either binary. Tracked as an orthogonal C/C++ action
        codegen fix (affects every platform), not a 195.D item. *(Recovery note: the
        dev tree's `third-party/` + `build/` were destroyed mid-session by a stray
        `rm -rf` in a worktree script that ran in the main repo; restored via
        recursive submodule re-fetch + `just setup default` + `scripts/nuttx/build-nuttx.sh`.
        The accident, not the rewire, was the source of the prior "tree degradation".)*
  - [x] **195.D.3 — Install `nros` in setup + CI. DONE.** `scripts/install-nros.sh`
        (nano-ros-owned, pins the release, survives the submodule drop) is used by
        `bootstrap.sh`, `just setup`'s `cargo-tools`, the root `install-nros-cli`
        recipe, and the CI workflows (`ci.yml`, `dep-chain.yml`, `zephyr-dual-line.yml`
        all install the prebuilt + add `~/.nros/bin` to PATH instead of building the
        CLI from the submodule). The build assumes `nros` is provided (187.6).
  - [x] **195.D.4 — Re-cut a main-tracking `nros` release + pin it. DONE
        (`nros-v0.3.0`).** Cut from `nros-cli` main (97ff3fc — dd18511 + 195.C +
        the ci/fmt fixes); `release-binary.yml` published linux-x86_64 + linux-arm64
        + macos-arm64 (macos-x86_64 left source-fallback, flaky runner queue). SDK
        index `[tool.nros]` → 0.3.0 + sha256 (3 hosts); `install-nros.sh`
        `NROS_VERSION` → 0.3.0; submodule pointer → f2b3363. Verified install +
        `nros setup --list`.
  - [x] **195.D.5 — Remove the gitlink. DONE.** `git rm packages/codegen` + dropped
        the `.gitmodules` entry. Rewired every consumer to the installed `nros`:
        CI workflows (×3) install the release; `cargo.sh`/recipes resolve
        `$NROS_CLI`/PATH/`~/.nros/bin`; `cmake/NanoRosBootstrapCodegen.cmake` +
        `NanoRosGenerateInterfaces.cmake` + `tools/setup.sh` probe PATH/`~/.nros`;
        `CMakeLists.txt` dropped the submodule `add_subdirectory`/Corrosion-target
        branches; `config/submodule-deps.toml` no longer requires it; in-submodule
        test hooks removed (native.just codegen recipes, `test-audit-builds.sh`, the
        `cmake_*` test skip-conditions now gate on installed `nros`); root `justfile`
        Python format/check + install + clean-setup de-submoduled;
        `setup.{bash,fish}` export `~/.nros/bin`. `check-profile-board-mirror.sh`
        self-skips (its generator drift-check is nros-cli's concern now).
        **Verified:** `just nuttx build-fixtures` builds all Rust + C + C++ fixtures
        green with the submodule physically removed + the installed `nros`.
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

- [x] A fresh machine with **no Rust, no `just`, no checkout** can
      `curl …/install.sh | sh` → get `nros` → scaffold → build → run a first
      image. No source build of `nros`. **Verified in CI
      (`.github/workflows/nros-acceptance.yml`, run 26623384800, 2026-05-29):** a
      bare `ubuntu-22.04` runner with **no `actions/checkout`** installs the
      prebuilt `nros-v0.3.0`, then `nros new` → `nros build` (auto-provisions
      zenohd from the index) → runs the scaffolded binary — all steps green.
      (`nros deploy` is the `[deploy.<name>]`/`[workspace]` mode; the direct
      new→build→run chain is the bare-machine first-image proof.)
- [x] `nros setup <board>` provisions that board's `[source.*]` from **index
      data** into the index-declared `dest`; the same `nros` works for a different
      board's set with no rebuild. **Verified:** `nros setup qemu-arm-freertos
      --dry-run` resolves freertos-kernel + lwip (+3) → `third-party/...` from the
      index; the `dep-chain` CI exercises `nros setup` across 10 boards with one
      installed binary.
- [x] Editing a source `ref`/tool URL in `nros-sdk-index.toml` + passing
      `sdk-index-gate` fixes provisioning with **no `nros` rebuild** — the index
      is the SSOT (`.github/workflows/sdk-index-gate.yml` present; provisioning is
      index-driven, proven by the dry-run above).
- [x] The prebuilt `nros` version lands in `main` **only after** its per-host
      assets are published + sha-verified. **Followed for `nros-v0.3.0`:** the
      release CI published + sha'd the assets first, *then* the index `dist.*`
      sha256 + `install-nros.sh` pin were bumped (index sha == published asset sha,
      verified).
- [x] `nros` source has **no hardcoded `third-party/` provisioning path** — the
      `dest` paths come from `nros-sdk-index.toml` data (the dry-run shows
      `… → third-party/freertos/kernel` sourced from the index `[source.*].dest`),
      not baked into the binary. (Engine source now lives in nros-cli; its own CI
      owns the grep-clean guard.)

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
