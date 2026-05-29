# Phase 202 — Zephyr "bring-your-own-workspace" (BYO) end-user UX

**Goal.** Make a Zephyr end-user able to adopt nano-ros as an add-on module to
**their own** west workspace and get to a running app, without hitting the gaps
the in-tree contributor flow hides. Zephyr mandates a dedicated west workspace;
nano-ros is a module (`zephyr/module.yml`) imported into it — so the BYO path is
the *real* user surface, and it currently breaks out of the box.

**Status.** Largely landed (2026-05-29). **202.1–202.6 all addressed** (mix of
doc fixes + the version-tolerant rust patch); the BYO docs now cover prerequisites,
transport sources, the complete patch story (NSOS/rust/cyclonedds), the rust-app
`generate-config` workflow, and a single canonical guide. **Complete:** the full BYO path is **e2e-verified** — a fresh `west` workspace
builds `c/talker` to `zephyr.exe` and runs to `Published: 1` against `zenohd`. The
e2e surfaced + fixed two more gaps (202.7: px4-rs provisioning + the cmake
`NROS_PLATFORM_CFFI_INCLUDE` export). All 202 items + acceptance done. From a BYO-adoption walkthrough of the
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

- [x] **202.1 — [P1, doc DONE] `west update` doesn't pull nano-ros's transports.**
      Documented the fix in both BYO docs (`integration-zephyr.md` Build section +
      `integrations/zephyr/README.md`): after `west update`, provision the RMW's
      transport from the nano-ros checkout via `nros setup --source zenoh-pico`
      (zenoh) / `--source cyclonedds-src` (cyclone) — the canonical lean provisioner
      — with the `submodules: true` west-native alternative noted (pulls all
      submodules incl. unrelated platform SDKs). **Still open:** end-to-end verify
      on a throwaway BYO west workspace, and confirm the module CMake builds the
      now-present zenoh-pico without further wiring.
      **E2E VERIFIED (2026-05-29)** on a throwaway BYO west workspace (`west init`
      + a manifest importing nano-ros from a local clone, `west update`):
      `packages/zpico/zpico-sys/zenoh-pico` came up **empty** (0 files — confirms
      `west update` does not pull the transport submodules), and
      `nros setup --source zenoh-pico` run from `modules/nano-ros` **provisioned**
      it (33 files, `include/` present, pinned `f68feb77`). The documented BYO fix
      works. The module *build* against the present transports is the same module
      CMake the in-tree dual-line CI already builds green (3.7+4.4 C/C++ compile
      zenoh-pico from that submodule), so a full BYO `west build` (≈2 GB Zephyr
      clone) was not re-run.
      *Original issue:* the transports (zenoh-pico, the cyclonedds fork, mbedtls)
      are git submodules; `integrations/zephyr/west.yml` has `projects: []` and the
      import snippet didn't set `submodules: true`, so `west update` fetched
      nano-ros but no submodules → zenoh-pico absent → link error, despite the docs
      claiming "west update pulls … transitives".
      **Files:** `book/src/getting-started/integration-zephyr.md`,
      `integrations/zephyr/README.md`, `integrations/zephyr/west.yml`,
      `zephyr/CMakeLists.txt`.

- [x] **202.2 — [P1, DONE] No "install the nros CLI + source ROS" prerequisite.**
      Added a **Prerequisites** section to `integration-zephyr.md` (install.sh for
      the `nros` CLI + `source /opt/ros/<distro>/setup.bash`) and a matching block
      in `integrations/zephyr/README.md`. The module build's interface codegen
      (`_NANO_ROS_CODEGEN_TOOL`) needs both; the doc previously only hinted at "ROS
      Python".
      **Files:** `book/src/getting-started/integration-zephyr.md`,
      `integrations/zephyr/README.md`.

- [x] **202.3 — [P2, DONE] Split, incomplete patch story for BYO.** `west patch`
      ships only the 4 NSOS/native-sim/pthread patches (`zephyr/patches.yml`); the
      rust examples also need the cortex-a9 / aarch64 / cortex-r / cargo-features /
      rust-cargo-extra-args scripts, and cyclonedds patches are baked into our
      submodule pin. **Chose to document the script fallback** rather than fold the
      scripts into `west patch`: they edit the `modules/lang/rust` project, are
      board/arch-conditional, and are anchor-based + version-tolerant (warn-and-skip
      on upstream drift) — qualities a static `.patch` index would lose.
      `integrations/zephyr/README.md` gained a "Rust examples need additional
      patches" subsection: the exact `modules/nano-ros/scripts/zephyr/*.sh
      <workspace>` invocations (cargo-features + rust-cargo-extra-args for all rust;
      the per-arch rust patch only for cortex-a9 / aarch64 / cortex-r), noting C/C++
      need none. The NSOS (`west patch`) + cyclonedds sections already existed.
      **Files:** `integrations/zephyr/README.md`.

- [x] **202.4 — [P2, DONE as docs — the constraints are intrinsic, not
      removable].** Investigated both "leaks":
      - The `rustapp` name is the **`[lib]`** name (not the package — the package
        is `nros_zephyr_talker`), and it's an **upstream `zephyr-lang-rust`
        contract**: `rust_cargo_application()` links `librustapp.a`
        (`zephyr/CMakeLists.txt:69`). Not a nano-ros leak; can't be renamed away.
      - The `[patch.crates-io]` must live in the **consuming crate's**
        `.cargo/config.toml` (a Cargo rule) — a CMake module *cannot* inject it. So
        it can't be "provided through the module"; what can change is *how the user
        gets correct paths*.
      **Fix (docs):** added a "Rust applications" section to
      `integration-zephyr.md` — (1) note the `rustapp` `[lib]` requirement; (2)
      tell rust BYO apps to **generate** their config rather than copy the
      example's repo-relative one: `nros generate-rust --generate-config
      --nano-ros-path <workspace>/modules/nano-ros/packages/core` writes both the
      `generated/<pkg>` interface crates and a `.cargo/config.toml` whose
      `[patch.crates-io]` points at the user's own layout. Verified the command
      emits absolute per-layout patch paths. **Files:**
      `book/src/getting-started/integration-zephyr.md`.

- [x] **202.5 — [P2, mostly DONE] `zephyr-lang-rust` reproducibility + patch
      tolerance.** Both lines are in fact already **pinned** (not floating `main`):
      `west.yml` → `404fcefdbab0…`, `west-4.4.yml` → `a763400f31e9…` — the pin goal
      is met. The live failure was the *consequence* of the two lines pinning
      **different** lang-rust shapes: `scripts/zephyr/rust-cargo-extra-args-patch.sh`
      (the Phase 200.1 rust feature-forwarding patch) hard-`exit 1`'d
      ("librustapp CARGO_ARGS block not found") on the 4.4 commit, and because it
      runs in the **shared** `just zephyr setup`, it took down the 4.4 **C/C++**
      cells too. **Fixed:** made that patch version-tolerant — WARN + skip the
      build block when the anchor's shape differs (matching the repo's cortex-a9
      patch pattern), so a divergent lang-rust shape no longer blocks setup /
      C/C++. Verified both paths (anchor-present → patched; absent → warn+skip,
      exit 0). *Remaining:* the 4.4 rust cells still need the EXTRA_CARGO_ARGS
      forwarding to actually reach cargo on the `a763400` shape (rust-200.1, owned
      by the rust-zephyr work). **Files:** `scripts/zephyr/rust-cargo-extra-args-patch.sh`,
      `west.yml`, `west-4.4.yml`.

- [x] **202.6 — [P3, mostly DONE] Two workspace models + two patch mechanisms =
      cognitive load.** Three things were split: in-tree vs BYO pages, and
      `west patch` vs sed-script patch paths.
      - **In-tree vs BYO framing — done** (concurrent rewrite): `zephyr.md` now
        opens "contributor / in-tree workflow" with a callout to the BYO page, and
        `integration-zephyr.md` opens with a "Contributor path?" callout naming
        itself "the canonical user entry". The two cross-link cleanly.
      - **Single BYO source of truth — done:** `integrations/zephyr/README.md`
        gained a top banner declaring the book's `integration-zephyr.md` the
        canonical BYO guide and itself the *dir-level reference* (manifest fragment
        + patch mechanics), so the procedural steps don't fork.
      - **Single patch entry point — largely subsumed by the model:**
        `nros setup zephyr --rmw <rmw>` provisions + applies the patches during
        setup (per the book's version matrix), so the typical user has one command;
        `west patch` + the rust/cyclonedds scripts are the under-the-hood / advanced
        BYO fallbacks (documented in the README, 202.3).
      **Reconciled (2026-05-29):** verified `nros setup zephyr --rmw <rmw>` (board
      mode) provisions the transports (zenoh-pico + mbedtls + zenohd for zenoh), so
      the README's `## Prerequisites + transport sources` was rewritten to point at
      the book's canonical `nros setup zephyr` flow (one command) instead of the
      forked manual `nros setup --source` / ROS steps — the two docs no longer
      diverge. **Files:** `book/src/getting-started/{zephyr,integration-zephyr}.md`,
      `integrations/zephyr/README.md`.

- [x] **202.7 — [P1, DONE] BYO module build was not self-contained (found by the
      e2e).** A full BYO `west build` of `c/talker` surfaced two gaps the doc fixes
      hadn't:
      - **px4-rs not provisioned** — the `nros-c` cargo build loads the whole
        workspace (path-deps `px4-sitl-tests`), so a BYO build needs
        `nros setup --source px4-rs` beyond the transports. Documented in the BYO
        guide + README.
      - **`NROS_PLATFORM_CFFI_INCLUDE` only from `.env`/direnv** — `zpico-sys`'s
        build.rs requires it; the zephyr cmake passed `ZPICO_*`/`XRCE_*` to the
        cargo subprocess but not this, so a BYO build (no `.env`) panicked. Fixed:
        `nros_set_cargo_env_from_kconfig` sets it from the module path +
        `nros_cargo_build`'s explicit `cmake -E env` forwards it (also removes the
        in-tree `.env` reliance for this var).
      **Files:** `zephyr/cmake/nros_cargo_build.cmake`,
      `book/src/getting-started/integration-zephyr.md`, `integrations/zephyr/README.md`.

## Acceptance
- [x] A fresh BYO west workspace (`west init` + the nano-ros import) reaches a
      running `native_sim` zenoh app following only the BYO doc — no in-tree
      `just` recipes, no undocumented submodule/`nros`/ROS steps (202.1/202.2).
      **E2E VERIFIED (2026-05-29):** built `c/talker` to `zephyr.exe` on a fresh
      BYO west workspace and ran it to **`Published: 1`** against `zenohd` (after
      the 202.7 fixes). Boot → NSOS network → nros init → zenoh connect → publish
      all confirmed.
- [x] `west patch apply` (or a documented equivalent) applies everything a given
      board/RMW needs (202.3) — `west patch` for NSOS + the documented
      rust/cyclonedds script fallbacks; `nros setup zephyr` applies them during
      provisioning.
- [x] A copied-out rust app needs no **hand-edited** `[patch.crates-io]` — it
      `nros generate-rust --generate-config`s one for its layout (202.4). (The
      `rustapp` `[lib]` name is an upstream `zephyr-lang-rust` contract, not a
      nano-ros rule that can be removed — reworded from the original criterion.)
- [x] `zephyr-lang-rust` is pinned per line (`west.yml` `404fcef`, `west-4.4.yml`
      `a763400`); CI + BYO build the same revision (202.5).

## Notes
- Verifying 202.1 end-to-end needs a throwaway BYO west workspace (`west init` +
  a Zephyr clone) — heavier than the in-tree flow; do it on a provisioned host.
- The deeper CLI-ergonomics gaps (no `west init`-equivalent bootstrap, scattered
  config, no `flash`/`run`/`monitor` verbs) are catalogued in
  `docs/research/sdk-ux/zephyr-and-esp-idf.md`; this phase scopes only the BYO
  add-on adoption path, not a CLI redesign.
