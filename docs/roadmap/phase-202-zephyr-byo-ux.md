# Phase 202 — Zephyr "bring-your-own-workspace" (BYO) end-user UX

**Goal.** Make a Zephyr end-user able to adopt nano-ros as an add-on module to
**their own** west workspace and get to a running app, without hitting the gaps
the in-tree contributor flow hides. Zephyr mandates a dedicated west workspace;
nano-ros is a module (`zephyr/module.yml`) imported into it — so the BYO path is
the *real* user surface, and it currently breaks out of the box.

**Status.** Proposed (2026-05-29). From a BYO-adoption walkthrough of the
manifest/module/docs (`integrations/zephyr/`, `book/src/getting-started/
integration-zephyr.md`). Complements the broader CLI-verb UX study
`docs/research/sdk-ux/zephyr-and-esp-idf.md` (2026-05-04) — this phase is the
BYO-specific subset + fixes.

**Priority.** P2 — adoption blocker for external Zephyr users, but no in-tree
capability depends on it (the contributor flow + CI use the in-tree workspace).
202.1 (submodules) is the one that makes BYO build at all.

**Depends on.** Phase 139 (the `integrations/zephyr/` shell), Phase 180.C/.D
(snippets + `west patch`), Phase 195/197 (the `nros` CLI + `nros setup`).

## Overview

The documented BYO flow (`integration-zephyr.md` / `integrations/zephyr/README.md`):
1. add nano-ros to `west.yml` (import `integrations/zephyr/west.yml`) → `west update`
2. `west patch apply` (NSOS patches)
3. `CONFIG_NROS=y` + `CONFIG_NROS_RMW` in `prj.conf`
4. `west build -b <board> apps/my_app` → run → verify against stock ROS 2

Walking it surfaces six issues. The in-tree contributor flow papers over them
because `just zephyr setup` runs `nros setup --source` + the patch scripts + the
codegen tooling — none of which a BYO west user invokes.

## Work items

- [ ] **202.1 — [P1] `west update` doesn't pull nano-ros's transports (BYO build
      link-fails out of the box).** The transports — zenoh-pico
      (`packages/zpico/zpico-sys/zenoh-pico`), the cyclonedds fork, mbedtls — are
      **git submodules**; `integrations/zephyr/west.yml` has `projects: []`, and
      the documented import snippet does **not** set `submodules: true` on the
      nano-ros project. So `west update` fetches nano-ros but none of its
      submodules → zenoh-pico absent → link error. The docs claim "west update
      pulls nano-ros + transitives" — it doesn't. **Fix:** document `submodules:
      true` on the nano-ros project entry (and verify the module CMake builds the
      now-present zenoh-pico), or have the module provision its sources. This is
      the one that makes BYO actually build.
      **Files:** `book/src/getting-started/integration-zephyr.md`,
      `integrations/zephyr/README.md`, `integrations/zephyr/west.yml`,
      `zephyr/CMakeLists.txt`.

- [ ] **202.2 — [P1] No "install the nros CLI + source ROS" prerequisite in the
      BYO doc.** The module build invokes the interface codegen
      (`_NANO_ROS_CODEGEN_TOOL`), which needs the released `nros` (install.sh) AND
      a sourced ROS 2 (`AMENT_PREFIX_PATH`) to resolve a message package's
      `msg/*.msg`. The BYO doc only hints at "ROS Python"; it never tells the user
      to install `nros` first. **Fix:** add an explicit prerequisite block
      (install.sh + `source /opt/ros/<distro>/setup.bash`) to the BYO doc.
      **Files:** `book/src/getting-started/integration-zephyr.md`.

- [ ] **202.3 — [P2] Split, incomplete patch story for BYO.** `west patch` ships
      only the 4 NSOS/native-sim/pthread patches (`zephyr/patches.yml`). Rust
      examples additionally need the cortex-a9 / aarch64 / cortex-r / cargo-features
      sed scripts (`scripts/zephyr/*.sh`); the cyclonedds patches are baked only if
      the user vendors *our* `third-party/dds/cyclonedds` submodule. A BYO
      rust/non-native_sim user hits un-applied patches with no single command.
      **Fix:** either fold the rust/cargo-features patches into `west patch`
      (`patches.yml`) so `west patch apply` is complete, or document the script
      fallback per board/RMW. **Files:** `zephyr/patches.yml`, `zephyr/patches/`,
      `scripts/zephyr/*-patch.sh`, the BYO doc.

- [ ] **202.4 — [P2] nano-ros internals leak into the user's rust project.** Every
      rust example forces `[patch.crates-io]` into its `.cargo/config.toml` + a
      "package name must be `rustapp`" rule (carried over from the 2026-05-04 UX
      study). A copied-out user app inherits both. **Fix:** remove/relax the
      `rustapp` name constraint and provide the interface/`[patch]` wiring through
      the module rather than per-app `.cargo/config.toml`. **Files:**
      `examples/zephyr/rust/*/Cargo.toml`, `examples/zephyr/rust/*/.cargo/config.toml`,
      `zephyr/cmake/`.

- [ ] **202.5 — [P2] `zephyr-lang-rust` pinned to a floating `main` → recurring
      build breaks.** `west.yml` / `west-4.4.yml` pin `zephyr-lang-rust` at
      `revision: main`. Upstream API churn (e.g. `export_bool_kconfig` →
      `export_kconfig_bool_options`, Phase 200 build.rs fix) silently breaks the
      rust examples — and a BYO user on a different `main` snapshot gets *different*
      breakage. **Fix:** pin `zephyr-lang-rust` to a tested commit (per Zephyr
      line) so BYO + CI are reproducible; bump deliberately. **Files:** `west.yml`,
      `west-4.4.yml`, `examples/zephyr/rust/*/build.rs`.

- [ ] **202.6 — [P3] Two workspace models + two patch mechanisms = cognitive
      load.** The story is split across `book/src/getting-started/zephyr.md`
      (in-tree), `integration-zephyr.md` (BYO), and `integrations/zephyr/README.md`,
      with `west patch` vs the sed scripts as parallel patch paths. **Fix:** a
      single BYO quickstart that links the in-tree page as "contributors only", and
      one patch entry point. **Files:** the three docs above.

## Acceptance
- [ ] A fresh BYO west workspace (`west init` + the nano-ros import) reaches a
      running `native_sim` zenoh app following only the BYO doc — no in-tree
      `just` recipes, no undocumented submodule/`nros`/ROS steps (202.1/202.2).
- [ ] `west patch apply` (or a documented equivalent) applies everything a given
      board/RMW needs (202.3).
- [ ] A copied-out rust app builds without the `rustapp` name rule or hand-edited
      `[patch.crates-io]` (202.4).
- [ ] `zephyr-lang-rust` is pinned; CI + BYO build the same revision (202.5).

## Notes
- Verifying 202.1 end-to-end needs a throwaway BYO west workspace (`west init` +
  a Zephyr clone) — heavier than the in-tree flow; do it on a provisioned host.
- The deeper CLI-ergonomics gaps (no `west init`-equivalent bootstrap, scattered
  config, no `flash`/`run`/`monitor` verbs) are catalogued in
  `docs/research/sdk-ux/zephyr-and-esp-idf.md`; this phase scopes only the BYO
  add-on adoption path, not a CLI redesign.
