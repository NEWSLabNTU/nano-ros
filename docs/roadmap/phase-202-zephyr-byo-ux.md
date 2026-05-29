# Phase 202 — Zephyr "bring-your-own-workspace" (BYO) end-user UX

**Goal.** Make a Zephyr end-user able to adopt nano-ros as an add-on module to
**their own** west workspace and get to a running app, without hitting the gaps
the in-tree contributor flow hides. Zephyr mandates a dedicated west workspace;
nano-ros is a module (`zephyr/module.yml`) imported into it — so the BYO path is
the *real* user surface, and it currently breaks out of the box.

**Status.** In progress (2026-05-29). **202.1 + 202.2 (the P1 build-blockers)
done as doc fixes** — the BYO docs now cover transport-source provisioning
(`nros setup --source`) + the nros-CLI/ROS prerequisites. 202.3–202.6 (patch
completeness, rust-app internals leak, `zephyr-lang-rust` pin, doc consolidation)
remain; 202.1's end-to-end verify on a real BYO workspace is still open. From a
BYO-adoption walkthrough of the
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

- [ ] **202.4 — [P2] nano-ros internals leak into the user's rust project.** Every
      rust example forces `[patch.crates-io]` into its `.cargo/config.toml` + a
      "package name must be `rustapp`" rule (carried over from the 2026-05-04 UX
      study). A copied-out user app inherits both. **Fix:** remove/relax the
      `rustapp` name constraint and provide the interface/`[patch]` wiring through
      the module rather than per-app `.cargo/config.toml`. **Files:**
      `examples/zephyr/rust/*/Cargo.toml`, `examples/zephyr/rust/*/.cargo/config.toml`,
      `zephyr/cmake/`.

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
