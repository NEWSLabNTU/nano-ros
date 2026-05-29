# Phase 196 — CI bring-up + hardening for nano-ros

**Goal.** Give nano-ros a reliable, **live-validated** CI surface. Today CI is
thin and partly broken: the root `ci.yml` only fans out one crate on one target,
`zephyr-dual-line.yml` had never run live (7 stacked bring-up bugs), and several
workflows assume host state (submodules, ROS, SDKs) the GitHub runners don't
have. This phase finishes the Zephyr dual-line bring-up, audits the other
workflows for the same class of gaps, and codifies the provisioning conventions
so a new workflow works on its first push.

**Status.** In progress (2026-05-28). `zephyr-dual-line` setup + most build
stages fixed and validated by repeated `workflow_dispatch` runs; one product
skew (codegen CLI) remains, owned by Phase 195. The broader audit/codification
items are proposed.

**Priority.** P2 — no product capability depends on it, but green CI is the gate
for trusting every other phase's "verified" claim, and broken-by-default
workflows train contributors to ignore CI.

**Depends on.** Phase 180 (the dual-line workflow + recipes), Phase 195 (the
in-flight `nros codegen` CLI migration — the one open item rides on it).

---

## Background — the dual-line bring-up (what this phase already did)

`zephyr-dual-line.yml` (Phase 180.A Task 10) shipped CI-only, explicitly **never
run live**. The first live runs failed 100%, in stacked layers (each CI pass
≈8–16 min revealed the next). All landed on `feature/phase-172`:

1. **Workspace created inside the SDK dir.** `WORKSPACE_DIR` was relative;
   `install_sdk` `cd`s into the SDK and never returns, so the later relative
   `cd "$WORKSPACE_DIR"` landed in `scripts/zephyr/sdk/…`. Only triggers on a
   fresh install (SDK build runs) — why local cached-SDK runs passed.
   Fix: normalize `WORKSPACE_DIR` to absolute while cwd is the repo root
   (`scripts/zephyr/setup.sh`).
2. **`cortex-a9-rust-patch.sh` hard-fails on 4.4.** Zephyr 4.4 relocated the
   Zynq-7000 SoC (`soc/xlnx/zynq7000/xc7zxxxs/soc.c`). Fix: gate the patch to the
   3.7 manifest in `setup.sh`; make the patch version-tolerant (missing SoC →
   warn+skip, not `exit 1`).
3. **Stray committed `zephyr-workspace` symlink** (→ `../nano-ros-workspace`,
   broken on CI) — `mkdir -p` errors on a non-dir. Untracked it (it was already
   gitignored); guarded the mkdir with `[ -d ] ||`.
4. **Zephyr 4.4 needs Python ≥3.12.** `provision-py312-venv.sh` requires `uv`
   (no fallback); runner lacked it. Fix: `astral-sh/setup-uv@v5` step.
5. **Submodules not initialized + a needless XRCE-agent build.**
   `actions/checkout` inits no submodules; the 3.7 cyclone patches need
   `third-party/dds/cyclonedds`, the zenoh build needs
   `packages/zpico/zpico-sys/zenoh-pico`, and the setup recipe's common tail ran
   `just xrce setup` (a Fast-DDS superbuild) on every job though the workflow
   only builds zenoh. Fix: init just the needed submodules; gate the agent build
   behind `NROS_ZEPHYR_SKIP_XRCE_AGENT` (set workflow-wide).
6. **`packages/codegen` submodule not initialized** — `build-one` builds the
   host codegen tool from it. Added to the init step.
7. **No ROS 2 on the runner.** The interface codegen resolves `std_msgs`'s
   `msg/*.msg` via `AMENT_PREFIX_PATH` from a sourced ROS 2. Fix: jammy runner
   (Humble baseline) + `ros-tooling/setup-ros@v0.7` + `source
   /opt/ros/humble/setup.bash` before each build.

Result: setup passes on **both lines**; builds reach the interface codegen.

---

## Work Items

### 196.1 — [DONE] Finish the dual-line build: codegen CLI skew
The build now fails at the codegen call:
`nros-codegen failed for std_msgs (exit 2): error: unexpected argument '--args-file'`.
The codegen CLI moved to a `nros codegen` subcommand (Phase 195 `27e9be2`/
`07e3339`, "make `nros codegen` canonical") and 195.D switched in-tree
consumers — but **`zephyr/cmake/nros_generate_interfaces.cmake` still invokes
`nros --args-file …` without the `codegen` subcommand** (lines 305/308). Two
coupled facts:
- the cmake consumer was missed by the 195.D consumer-switch;
- the `packages/codegen` submodule pointer is **drifting** (superproject records
  `624e5bc6`, a local working tree sat at `860f301`).

- [x] **DONE.** `zephyr/cmake/nros_generate_interfaces.cmake` now invokes
      `nros codegen --args-file …` / `nros codegen --language cpp --args-file …`
      (the canonical `nros codegen` subcommand). Verified `nros codegen
      --args-file` parses. It was the only superproject `--args-file` consumer
      (the canonical cmake module lives in `packages/codegen`).
- [x] **Submodule pointer reconciled + the 195.D codegen-workspace bug fixed:**
      195.D deleted the `nros-codegen-c` crate dir but left it in
      `packages/Cargo.toml` `[workspace].members` → the whole codegen workspace
      failed to load (broke every `nros` CLI build). Dropped the stale member
      (codegen `00a1b2d`, pushed); bumped the superproject pointer. nros builds
      again.

### 196.2 — [DONE] `nros codegen` consumer-coverage check
195.D switched consumers to `nros codegen` but missed the Zephyr cmake (196.1).
A guard now makes that exact regression un-mergeable.

- [x] **DONE.** `scripts/ci/codegen-invocation-check.sh` — a static lint
      (`git ls-files` + `grep`, no toolchain/ROS) over superproject build glue
      (`*.cmake`/`CMakeLists.txt`/`*.just`/`justfile`/`*.sh`/`*.rs`, excluding
      `third-party/` + the `packages/codegen/` submodule). The signature is
      precise: any line driving the codegen tool with `--args-file` MUST carry
      the `codegen` subcommand token first; the legacy top-level `--args-file`
      (the 196.1 bug) fails. User-facing verbs (`nros generate-rust`,
      `nros generate cpp`) don't use `--args-file`, so they're untouched.
      Wired as `.github/workflows/codegen-convention.yml` (push/PR on
      cmake/just/scripts paths). Verified: clean tree OK, canonical form OK,
      injected legacy form exits 1 with the offending line.

**Files**: `scripts/ci/codegen-invocation-check.sh`,
`.github/workflows/codegen-convention.yml`.

### 196.3 — [mostly DONE] Audit/split the workflows (core-libs + per-platform)
The dual-line bugs (host assumes: submodules, ROS, SDK, Python, runner OS) are
generic. Audit each workflow with a fresh-runner lens; each must be live-run
once before being trusted:
- [x] `ci.yml` → **core-libraries lane** (DONE). **Scope decision** (maintainer,
      2026-05-29): *split CI into several parts — a core-libraries lane + one lane
      per platform, each pulling only its own tools/submodules so per-workflow
      minutes stay low.* `ci.yml` is now the **core-libs** lane: the portable
      `no_std` core crates cross-checked on bare embedded targets, fresh-runner-
      safe (no SDK/submodule deps, so no provisioning beyond a rustup target).
      Split by target (one job per target, parallel + isolated), each running a
      single `cargo check` over the compatible crates. Two targets:
      `thumbv7m-none-eabi` (atomic CAS — full set incl. `nros-rmw-cffi`) and
      `riscv32imc-unknown-none-elf` (no CAS — drops `nros-rmw-cffi`; that exact
      capability split is what the lane guards). Crates: nros-core, nros-log,
      nros-serdes, nros-params, nros-platform-api, nros-platform-cffi,
      nros-platform-critical-section, nros-rmw (+ nros-rmw-cffi on CAS targets).
      Verified both combined checks pass locally. Triggers broadened to
      `packages/core/**`.
- [x] **Per-platform lanes** (architecture recorded; buildout = follow-up). The
      split is realized structurally today by `zephyr-dual-line.yml` (the first
      per-platform build lane: pulls only Zephyr's SDK + cyclonedds/zenoh-pico
      submodules) and `dep-chain.yml` (the cheap cross-platform *resolution*
      cut — `nros setup` per board pulls only that board's tools, one ROS install
      shared). Adding a dedicated **build** lane per remaining platform
      (freertos, nuttx, threadx, esp32, bare-metal, stm32f4) on the dual-line
      pattern is follow-up work — each its own workflow scoped to its tools.
- [x] **Host unit-test lane** (DONE, 2026-05-29). Added `host-unit-tests.yml`:
      `just test-unit` (workspace unit tests — `cargo nextest --workspace
      --exclude nros-tests`) on a fresh runner. Closes the gap where **no** lane
      executed the host test suite (core-libs only `cargo check`s the no_std
      crates, dep-chain only resolves, the platform lanes build/boot fixtures) —
      a unit-level regression could land green. Fresh-runner-safe: prebuilt nros +
      `nros setup --source` for the workspace build sources the `*-sys` crates
      compile (px4-rs, zenoh-pico, mbedtls, micro-cdr, micro-xrce-dds-client) — no
      QEMU/zenohd/ROS/agent. Verified green on the runner (422/422 unit tests).
- [x] **Submodule provisioning is nros-driven, not hand `git submodule`**
      (DONE, 2026-05-29 — maintainer directive: "use nros tools to pull
      toolchains and submodules to simulate user workflow; if missing, fix nros
      and config"). Every lane now pulls sources the way a user does — build the
      `nros` CLI, then `nros setup <board>` / `nros setup --source <name>` —
      with the *only* hand-init being `packages/codegen` (the CLI bootstrap;
      chicken/egg). Index gaps fixed as "missing config": added
      `[source.px4-rs]` (core-libs workspace-load dep) and
      `[source.cyclonedds-src]` (the in-tree cyclonedds fork the Zephyr setup
      patches; distinct from `[tool.cyclonedds]`). `ci.yml` provisions px4-rs;
      `zephyr-dual-line.yml` (all 3 jobs) provisions zenoh-pico + cyclonedds-src;
      `dep-chain.yml` was already board-driven. Verified each `--source` resolves
      + the structure gate passes. Rule codified in `ci-conventions.md`.
- [x] `deploy-book.yml` — confirmed building (GREEN on `88488d296`, 2026-05-29);
      `cancel-in-progress: false` concurrency added (196.5) so a Pages deploy
      isn't interrupted mid-flight.
- [x] `sdk-index-gate.yml` — DONE (2026-05-29). `verify-index.py` gained an
      **offline structure pass** (always runs, alongside the network `dist`
      hash check; `--structure-only` for local/CI-without-network): (1) every
      `[board.*]`/`[rmw.*]` (Phase 191.6) package ref resolves to a defined
      `[tool]`/`[source]`/`[gated]` entry (static mirror of `SdkIndex::validate`,
      no `nros` build); (2) `[source.*]` (195.B) coherence — submodule mode needs
      `dest`, clone mode needs `ref`+`dest`; (3) **index↔`.gitmodules` drift
      guard** — each `[source.*].submodule` path must be a real submodule and a
      declared `git` URL must match `.gitmodules`. Gate now also triggers on
      `.gitmodules` changes. Verified: passes on the real index; catches
      undefined-ref, clone-missing-ref, missing-submodule-path, and URL-drift.
- [x] `zephyr-dual-line.yml` — 196.1 fixed. **SDK caching reverted** (added then
      backed out, 2026-05-29): caching `scripts/zephyr/sdk` restores the SDK
      *files* but not its CMake-package registration (`~/.cmake/packages/Zephyr-sdk`,
      written by the SDK's `setup.sh -c`), and `setup.sh` *skips* re-registration
      when the SDK dir is already present — so a cache **hit** left the SDK
      unregistered and `find_package(Zephyr-sdk)` failed at configure (broke
      every example, C included). A correct cache must also restore/redo the
      registration (cache `~/.cmake/packages/Zephyr-sdk`, or export
      `ZEPHYR_SDK_INSTALL_DIR`, or unconditionally re-run `setup.sh -c`) — deferred
      to a proper 196.3 follow-up. West-workspace caching likewise deferred.

### 196.8 — [fixed, CI-confirming] Zephyr dual-line: rust/cpp example build gaps

**Two distinct root causes, both fixed (`fdcd1bb56`, `c791e4dac`):**
- **rust/\*** — `failed to load source for builtin_interfaces`: `just zephyr
  build-one` never ran `nros generate-rust` for rust examples, so the generated
  interface path-crates (std_msgs + transitive builtin_interfaces) the
  `.cargo/config.toml` patch points at didn't exist before the cargo-driven west
  build (the C/C++ paths codegen via `nros_generate_interfaces()` in cmake; rust
  had no equivalent). Fix: `build-one` runs `"$codegen_tool" generate-rust
  --force` in the example dir for `rust/*` (needs ROS sourced — AMENT). Reproduced
  the wipe→regen locally.
- **cpp/\*** — `node.hpp: fatal error: type_traits: No such file`: Phase
  189.M3.3.e added `#include <type_traits>` to nros-cpp `node.hpp`; Zephyr builds
  C++ `-ffreestanding`/picolibc (no C++ stdlib). Same class as the
  threadx-qemu-riscv64 shim (`c8a4bec2f`). Fix: add `zephyr/cxx-compat/type_traits`
  (enable_if + SFINAE is_convertible + integral_constant), which is already on the
  `CONFIG_NROS_CPP_API` include path next to the existing `<cstdint>`/`<utility>`
  shims.

Both surfaced only now because the build first had to clear provisioning + the
px4-rs workspace-load. CI run on `c791e4dac` confirming.

Original triage notes:
First full dual-line build that gets *past provisioning* (after the px4-rs
workspace-load fix, `64ca9f69a`): **C examples build green on both lines**
(`c/talker`, `c/listener` — 3.7 + 4.4), but **every `rust/*` and `cpp/*` example
fails** at the cargo build with `failed to load source for dependency
builtin_interfaces` (`run 26616922003`). Pre-existing / newly-surfaced, **not** a
provisioning regression — before the px4-rs fix every job died earlier at the
`px4-sitl-tests` workspace-load, so the rust/cpp lines had never reached their own
codegen step. The generated `builtin_interfaces` path-crate the rust/cpp examples
depend on isn't produced before cargo runs in the `just zephyr build-one`
rust/cpp flow (the C path codegens fine via `nros codegen` / cmake — 196.1). Needs
its own debugging pass on the dual-line rust/cpp interface-codegen wiring (likely
a missing `nros generate-rust` / `NROS_BUILTIN_INTERFACES_DIR` step for the
cargo-driven examples). The C line + all other lanes (core-libs, dep-chain,
codegen-convention, deploy-book, sdk-index-gate) are green.

### 196.4 — [DONE] Codify the CI provisioning conventions
- [x] **DONE.** `docs/development/ci-conventions.md` written: the "runner is a
      fresh clone" model + eight conventions with copy-paste step snippets —
      minimal submodule init (never recursive-all; *and* the don't-hand-init-what-
      `nros setup`-provisions exception), ROS provisioning (jammy + `setup-ros` +
      `source`), runner-OS-follows-distro (Humble ⇒ jammy), Python 3.12 via `uv`,
      build-the-`nros`-CLI-from-source (published bin is stale), canonical
      `nros codegen` invocation (196.2), `cancel-in-progress` concurrency (+ the
      "cancelled = dedup, not failure" note), and path-filter triggers. Plus a
      cost-discipline section (dep-chain vs full builds, SDK caching), the
      fail-loud precondition rule, and a worked-examples table mapping each
      convention to a live workflow.

### 196.5 — [DONE] Workflow trigger hygiene
`zephyr-dual-line` (and others) trigger on `packages/**` — nearly every push.
Combined with a broken workflow, that's constant red. Keep broad triggers (core
changes do affect Zephyr), but every workflow now dedups in-flight runs.

- [x] **DONE.** Audited all six workflows for `concurrency: cancel-in-progress`.
      Three were missing it: `ci.yml` + `sdk-index-gate.yml` now cancel in-flight
      per `${{ github.ref }}`; `deploy-book.yml` uses `group: deploy-book` with
      `cancel-in-progress: false` (a Pages deploy must not be interrupted
      mid-flight — serialize, don't cancel). `dep-chain`, `codegen-convention`,
      `zephyr-dual-line` already had it. All keep `workflow_dispatch` + scoped
      `paths:`; broad `packages/**` on the platform lanes is intentional and the
      concurrency dedup keeps it cheap.

### 196.6 — [DONE] Per-platform **dependency-chain** validation (light, not full builds)

**Distribution model (confirmed):** nano-ros ships as a **source release +
prebuilt host toolchains** (`nros setup` fetches the toolchains; the crates are
consumed from the source tree, **not** crates.io). So the dep chain per platform
is: `example → nros (path) + <backend> (path) + generated std_msgs (path, made
by codegen) + the prebuilt host toolchain (nros setup)`.

The goal of this CI is to **prove that dep chain resolves for every
`(platform, rmw)`**, *not* to run the heavy full-build matrix (that stays in the
sparse `just build-all` / `zephyr-dual-line` lanes). Per `(board, example, rmw)`:

- [ ] **Toolchain side:** `nros setup <board> --rmw <rmw> --dry-run` resolves the
      right prebuilt host tools (validates the `[board.*]`/`[rmw.*]` index wiring,
      Phase 191.6) — instant, no fetch.
- [ ] **Codegen step:** run `nros generate-rust` (or the build's codegen
      pre-step) so the example's `generated/std_msgs` path-crate exists.
      **Gotcha:** the examples declare `std_msgs = "*"`, a *generated* crate; with
      no codegen run, even `cargo tree` fails (`failed to select std_msgs … crates.io`).
      Codegen needs ROS-sourced `.msg` defs (`AMENT_PREFIX_PATH`), so this lane
      installs ROS like the dual-line.
- [ ] **Crate/feature side:** `cargo tree --target <triple> --no-default-features
      --features <combo> -e features` (resolution only — **no compile**) proves
      the feature graph pulls the right backend/platform crates, unifies, and the
      target-cfg deps line up. This is the cheap "dep chain correct" check.
- [ ] Matrix over the **board × rmw** cells from `examples/README.md` (the
      authoritative triple list). Each cell is seconds, so all platforms fit one
      cheap workflow; the full compile stays opt-in.

Distinct from the heavy lanes: this catches a broken feature/crate/toolchain
wiring (a missing optional dep, a feature that doesn't resolve on a target, a
board→toolchain typo) in seconds, without compiling every platform.

### 196.7 — [P2] Fix the dep convention for the source-release model
The three conventions (191.6 review) must collapse to the source-release reality:
- [ ] `nros new` scaffolds `nros = { version = "0.1" }` (crates.io — **the crates
      are not published**). Change it to the source-release dep: a path/relative
      reference into the installed source tree (or a documented `git =
      "https://github.com/NEWSLabNTU/nano-ros"` for out-of-tree projects) — match
      whatever the source-release layout actually places the crates at.
- [ ] README install line: wrong org + contradictory `--git`＋`--path`. Correct to
      `cargo install --git https://github.com/NEWSLabNTU/nano-ros nros-cli` (or the
      source-release's documented CLI install).
- [ ] The user-journey CI lane (196 background) builds a scaffolded project end to
      end, so this convention is exercised, not assumed.

### 196.9 — [in progress] Per-platform setup/build/test CI matrix
**Goal.** Consolidate CI around the user procedure *per platform* — the gap where
only Zephyr had a build lane (core-libs only `cargo check`s, dep-chain only
resolves, host-unit only runs workspace units). Each platform gets one lane:
`install prebuilt nros → just <plat> setup (nros-provisioned) → build → test`.

**Design (`.github/workflows/platform-ci.yml`).** One template, a **dynamic
matrix** over platforms:
- A `changes` pre-job (dorny/paths-filter) emits a JSON platform list → the
  `platform` job's `matrix.plat` (`fromJSON`). On a PR only the platforms whose
  code or the shared core (`packages/core`, `zpico`, `just`, `cmake`, the index)
  changed run; on `main`/dispatch, all. (Job-level `if` can't read `matrix`, so
  selection happens in the pre-job — the standard GHA pattern.)
- **Build** runs on push/PR (filtered); the heavier **QEMU test/e2e** is
  `workflow_dispatch`-only (`run_e2e`) — keeps runtime flakiness off every push ×
  N platforms (a nightly `schedule:` is the natural place to turn e2e on later).
- **Additive**, not a replacement: the cheap distinct gates stay — host-unit-tests
  (workspace units), core-libs (no_std cross-target), dep-chain (all-board
  *resolution*, incl. stm32f4/orin/fvp which get no full lane), and the bespoke
  zephyr-dual-line. Once these per-platform lanes are trusted, **dep-chain slims**
  (its resolution is subsumed for covered platforms) — the one real "replace".

**First live run (2026-05-29, run 26630807819, build-only).** The matrix structure
works: `changes` resolved the dynamic matrix, all cells ran (fail-fast off),
**nuttx built green**. The 6 others failed on wiring, now diagnosed:

- [x] **Matrix `plat` ↔ `just` module-name mismatch.** The matrix used hyphenated
      ids but the root `justfile` `mod` names differ: `qemu-baremetal` → **`qemu`**,
      `threadx-linux` → **`threadx_linux`**, `threadx-riscv64` → **`threadx_riscv64`**
      (underscores). `freertos`/`nuttx`/`esp32` already match (nuttx is the proof).
      Fix: matrix + path-filter keys use the exact `mod` names.
- [x] **`native` has no `setup` recipe** (`just native --list` → none) — it's the
      host platform (no SDK to provision). Decide: add a `native setup` (workspace
      sources only) or drop native from the matrix (host coverage = host-unit-tests
      + a native-examples build). Dropped from the matrix for now.
- [x] **Per-cell setup/build greening (fresh runner).** All 6 cells now build green
      (run 26633900068, build-only, 2026-05-29). `nros setup <board>` covers the
      build sources per 197.4-A; esp32's `setup` provisions sources + best-effort
      `[tool.esp32-qemu]` (the e2e emulator, not the build). qemu needed three more
      recipe/image fixes (below).
- [ ] **Wire e2e on a nightly `schedule:`** once cells build green, so QEMU runtime
      is exercised without blocking pushes. (A `workflow_dispatch -f run_e2e=true`
      already runs the `just <plat> test` step; the full-dispatch run 26632848732
      showed all e2e cells red — the heavier QEMU runtime / C/C++ coverage is the
      next item.)

**Review findings + maintainer directives (2026-05-29).**
- [x] **ROS-for-codegen gap (likely part of the 6 failing cells, beyond naming).**
      platform-ci installs **no ROS**, but the build step runs `nros generate-rust`
      for the rust examples (`just/freertos.just:75,148`, etc.), which resolves a
      package's `msg/*.msg` via `AMENT_PREFIX_PATH` from a **sourced ROS**.
      `./setup.bash` only PATHs the nano-ros tools (nros/qemu/zenohd/xrce-agent +
      SDK store) — it does **not** source ROS / set AMENT. So any cell building a
      rust example fails at codegen. (nuttx green is consistent — its cell either
      doesn't gen rust interfaces or its C path uses `NROS_*_DIR`, not AMENT.) The
      dep-chain + zephyr-dual-line lanes install ROS for exactly this reason.
- [x] **Directive: provision ROS via a prebuilt image, not per-cell `setup-ros`.**
      Done — `ci/docker/ci-base/Dockerfile` builds `ghcr.io/newslabntu/nano-ros-ci:humble`
      (`FROM ros:humble-ros-base` + cmake/ninja/dtc/gperf, uv, just, rustup + cross
      targets, gcc-arm-none-eabi, cargo-nextest, `safe.directory '*'`; published by
      `.github/workflows/build-ci-base-image.yml`). platform-ci's `platform` job runs
      `container:` against it (`shell: bash`, `packages: read` + GITHUB_TOKEN creds);
      all per-cell setup-ros/apt/rustup installs deleted. Image extras the cells
      surfaced: `libslirp0`+`libpixman` (prebuilt patched qemu links libslirp.so.0)
      and `ros-humble-example-interfaces` (qemu service/action codegen — ros-base
      ships only common_interfaces/std_msgs). The zephyr image `FROM nano-ros-ci`
      refactor is a follow-up (own item).
- [ ] **Directive: cover C/C++ tests too, not just rust/build.** The per-cell
      `test` step (`just <plat> test`) must exercise the C and C++ example/e2e
      paths, not only rust — each platform's c/cpp talker/listener (+ service where
      it exists) should run under QEMU like the zephyr c/cpp lanes do. Confirm
      `just <plat> test` includes the c/cpp fixtures (extend the recipe if it's
      rust-only).

**Container refactor + 6/6 build-green (2026-05-29, run 26633900068).** All six
platform cells (qemu, freertos, nuttx, threadx_linux, threadx_riscv64, esp32) build
green in the `nano-ros-ci:humble` container. freertos + threadx cells went green on
the baked ROS/AMENT alone; the two outliers needed recipe completion of the 197.4
`nros setup` rewrite + image deps:
- **esp32 `setup`** (`just/esp32.just`) provisions `nros setup esp32` (zenoh-pico/
  mbedtls); the Espressif QEMU fork is best-effort (`|| warn`) — build doesn't need
  the emulator, the e2e step gates on it.
- **qemu** (`just/qemu-baremetal.just`) took four fixes: (1) `setup` sources
  `setup.bash` so `build-zenoh-pico` uses the nros-provisioned arm-gcc 13.2 (newlib)
  not apt's headerless gcc-arm-none-eabi; (2) `libslirp0`+`libpixman` in the image
  for the prebuilt patched qemu; (3) `build` runs `nros generate-rust` for examples
  with a package.xml; (4) same codegen for bins with a package.xml (cdr-roundtrip-qemu).
- A non-fatal `cargo metadata`/cbindgen warning recurs (`px4-sitl-tests` workspace
  member unprovisioned — px4 is extended-tier); harmless to the bare-metal builds.

**Files.** `.github/workflows/platform-ci.yml`; the per-platform `just/<plat>.just`
setup/ci recipes; `nros-sdk-index.toml` (per-board package coverage);
`ci/docker/ci-base/Dockerfile` + `.github/workflows/build-ci-base-image.yml`
(the `nano-ros-ci` base image).

---

## Acceptance criteria
- [ ] `zephyr-dual-line` is green end-to-end on both lines (196.1).
- [ ] A codegen-consumer check fails on a stray `nros --args-file` / legacy form
      (196.2).
- [ ] Every `.github/workflows/*.yml` has had ≥1 successful live run on the
      current `main` (or a tracking branch), recorded here.
- [ ] `ci.yml` runs a meaningful workspace check, not a single-crate stub.
- [ ] A light **per-platform dep-chain** lane (196.6) green over the board × rmw
      matrix: `nros setup --dry-run` + codegen + `cargo tree --target`
      (resolution, no full build) for every cell.
- [ ] `nros new` scaffolds a dep that resolves under the source-release model
      (196.7); a scaffolded project builds in the user-journey lane.
- [ ] `docs/development/ci-conventions.md` exists and the dual-line + ci
      workflows follow it.

## Notes
- **Lesson (the expensive one):** a workflow that "has NOT been validated by a
  live GitHub Actions run" (its own header) is effectively broken. Validate via
  `gh workflow run --ref <branch>` before merging — the 7 dual-line layers were
  all first-run-only failures invisible to local runs (cached SDK, pre-init'd
  submodules, sourced ROS, dev symlinks).
- **Push/dispatch race seen during bring-up:** a `git push` that was rejected
  (branch behind) followed by `gh workflow run` dispatched the *stale* tip, so a
  "fixed" run silently re-ran the old code. Always confirm the push landed
  (`git fetch` + check the ref/headSha) before dispatching.
- The dual-line fixes live in `scripts/zephyr/setup.sh`,
  `scripts/zephyr/cortex-a9-rust-patch.sh`, `just/zephyr.just`,
  `.github/workflows/zephyr-dual-line.yml`, and the untracked `zephyr-workspace`
  symlink — all on `feature/phase-172`.
