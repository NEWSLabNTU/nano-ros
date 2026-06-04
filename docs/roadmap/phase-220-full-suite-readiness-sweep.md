# Phase 220 — Full-Suite Readiness Sweep (issues + regressions surfaced)

**Goal**: Bring `just build-test-fixtures` + `just test-all` back to clean
exit on a freshly-set-up host, capturing every issue + regression
surfaced during the 2026-06-04 sweep.

**Status**: LIVE. Created 2026-06-04 from 20 sequential `driver` runs
attempting to land BF-EXIT 0 + TA-EXIT 0. ~10 classes of breakage
discovered + partially fixed; full green still pending.

**Priority**: HIGH for tracks A–D (block test-all entirely on at
least one platform). MED for E–G (per-platform staging). LOW for
documentation + tooling polish (H+).

**Depends on**: Phase 210 (CLOSED), Phase 212 (mostly CLOSED), Phase 213
(CLOSED), Phase 214 (most tracks landed), Phase 218 (CLI in monorepo —
recently landed; affects setup flow).

---

## Overview

A 20-iteration `driver1..20` loop was run after a `git pull` brought
~15 fixes from another worker (H/I/J/K/L/F.1/F.2/S.5.c.4 etc.).
Each driver invoked `just build-test-fixtures` followed (when clean)
by `just test-all`. Every iteration surfaced 1–3 distinct breakages
that were patched in tree before the next iteration.

Resulting commit `4be1a4a41` ("fix(214): readiness sweep — env +
macro/include/typedef/path fixes for build-test-fixtures") aggregates
the mechanical patches. This phase doc captures the **deeper**
classes the sweep revealed:

| track | what | scope | severity |
|---|---|---|---|
| **A — Stale `~/.cargo/bin/nros` shadow** | install-nros.sh installs to `~/.nros/bin`, but a leftover `~/.cargo/bin/nros 0.2.0` from a long-ago `cargo install` outranks it on PATH | host env | HIGH |
| **B — N.12 rename sweep gaps** | `nros::EntityKind` removed from C++ API + `pub.id` → `pub.stable_id`, but threadx-linux cpp example src wasn't migrated | examples src | HIGH |
| **C — Phase 212.M.1/M.5.b deletion fallout** | `examples/native/rust/{talker,listener}/CMakeLists.txt` + `examples/qemu-arm-freertos/rust/talker/CMakeLists.txt` deleted; `just native build-fixtures` + `just freertos build-fixtures` cyclonedds-cmake path still tried to `cmake -S ...` them | recipes + missing fixture restore | MED |
| **D — zephyr workspace path drift** | `logging-smoke-zephyr-native-sim` recipe hardcodes `/home/aeon/repos/nano-ros/zephyr-workspace/zephyr/cmake/pristine.cmake`; post-`just zephyr setup` workspace lives at `nano-ros-workspace/` (sibling dir) | recipe path | HIGH |
| **E — Codegen stamp churn** | `nros ws sync` rewrites `[patch.crates-io]` blocks differently from earlier CLI versions; pinned-CLI vs source-built-CLI emit different patches; per-example regen wipes manual additions | tooling drift | MED |
| **F — `nros_node_options_t` collision** | hand-written `node_pkg.h` typedef collided with cbindgen-emitted runtime struct; rename to `nros_node_pkg_options_t` (landed via 4be1a4a41 + parallel commit on origin) | C ABI | HIGH (closed) |
| **G — `nano_ros_node_register` doesn't link interface lib** | nuttx + freertos C + threadx-linux cpp examples used `nano_ros_node_register(... SOURCES src/X.c ${pkg_GENERATED_SOURCES})` but the component lib target didn't propagate the `<pkg>__nano_ros_{c,cpp}` interface lib's include dirs. Each example needs an explicit `target_link_libraries(...PUBLIC <pkg>__nano_ros_{c,cpp})` appended OR the cmake fn should auto-link any `*_GENERATED_SOURCES`-providing lib | cmake fn | MED |
| **H — install-nros.sh retired post-218** | Phase 218 deleted `scripts/install-nros.sh`; `just setup-cli` now drives CLI install. Pre-218 driver scripts in this repo + my background drivers all still reference the deleted script | tooling cleanup | LOW |

Tracks A, B, C, D are CRITICAL — they block at least one platform's
fixture build outright. F + G are CLOSED (landed in `4be1a4a41`). E +
H are tooling polish.

---

## Architecture (one paragraph)

Almost every breakage traces to one of three root causes:
1. **Rename / deletion sweeps that left consumers behind** (B, C).
   N.12 + M.1 + M.5.b mass refactors removed APIs / files and didn't
   sweep every downstream consumer.
2. **Path drift between setup contracts** (A, D, E). The CLI install
   path moved (`~/.cargo/bin` → `~/.nros/bin`), the zephyr workspace
   moved (`zephyr-workspace/` → `nano-ros-workspace/`), and the
   codegen-emitted `[patch.crates-io]` block shape changed; recipes
   + scripts + previously-emitted artifacts still reference the old
   locations.
3. **Hand-written interfaces drifted from cbindgen-emitted runtime**
   (F, G). `node_pkg.h` typedefs collided with `nros_generated.h`;
   the `nano_ros_node_register` cmake fn doesn't propagate the
   generated interface lib's include dirs.

---

## Work Items

### A — Prune stale `nros` shadows on PATH (post-218)

* **Phase 218 install method (2026-06-04)**: `scripts/install-
  nros.sh` is RETIRED. Canonical install: `just setup-cli` builds
  the in-tree `packages/cli/` → `packages/cli/target/release/nros`,
  wired via `source ./activate.sh` (or direnv). The standalone
  `nros-cli` GitHub repo is ARCHIVED / read-only.
* **Symptom**: `which nros` → `~/.cargo/bin/nros 0.2.0` OR
  `~/.nros/bin/nros 0.3.x` (pre-218 install-nros.sh path). Both
  outrank the post-218 in-tree CLI in fresh shells without
  `activate.sh` sourced. Stale CLI emits stale `[patch.crates-io]`
  blocks (pre-K.7.1.b emit lacks `impl ::nros_serdes::Message`),
  breaking native rust cyclonedds builds.
* **Detection**: `just doctor` warns `[PATH] nros built at
  packages/cli/target/release/nros but not on PATH —` but doesn't
  FAIL on a stale shadow elsewhere.
* **Remedy**: extend `just doctor` to FAIL on any `nros` on PATH
  that isn't the in-tree `packages/cli/target/release/nros`; emit
  `rm ~/.cargo/bin/nros` / `rm -rf ~/.nros/bin` cleanup hints.

- [ ] **220.A.1** `just doctor` shadow-detection FAIL on any stale
      path (`~/.cargo/bin/nros`, `~/.nros/bin/nros`).
- [ ] **220.A.2** `just setup-cli` emits cleanup hint when stale
      shadows present.
- [ ] **220.A.3** Sweep agent driver scripts to source
      `./activate.sh` instead of prepending `~/.nros/bin`.

### B — Migrate threadx-linux cpp example src to post-N.12 API

* **Symptom**: `examples/threadx-linux/cpp/talker/src/Talker.cpp:28`:
  `pub.id = "pub_chatter";` → error: `'struct
  nros::NodeEntityDescriptor' has no member named 'id'; did you mean
  'kind'?`. Same file line 29: `pub.kind = nros::EntityKind::Publisher;`
  → `'nros::EntityKind' has not been declared`.
* **Detection**: surfaced when `just threadx_linux build-fixtures`
  attempted to compile the cpp examples (driver19/20).
* **Remedy**: 6 src files (talker/listener/service-{client,server}/
  action-{client,server}) need `pub.id` → `pub.stable_id` +
  `nros::EntityKind::*` → the post-N.12 spelling (likely
  `NodeEntityKind::PUBLISHER` or similar — confirm against the
  C++ header).

- [x] **220.B.1** Inventory the post-N.12 C++ entity-descriptor +
      kind-enum names by grep'ing
      `packages/core/nros-cpp/include/`. Confirmed in
      `declared_node.hpp`: `nros::NodeEntityKind` (variants
      `Publisher`/`Subscription`/`Timer`/`ServiceServer`/
      `ServiceClient`/`ActionServer`/`ActionClient`/`Parameter`);
      `NodeEntityDescriptor { stable_id, node_id, kind,
      source_name, type_name, type_hash, callback_id }`.
- [x] **220.B.2** Sweep `examples/threadx-linux/cpp/*/src/*.cpp` (6
      files) to the post-N.12 API. Mechanical replacements:
      `pub.id` → `pub.stable_id` (+ add `pub.node_id = "node"`),
      `nros::EntityKind::*` → `nros::NodeEntityKind::*`,
      `AddTwoInts::SERVICE_NAME`/`SERVICE_HASH` →
      `"example_interfaces/srv/AddTwoInts"` / `""`,
      `Fibonacci::ACTION_NAME`/`ACTION_HASH` →
      `"example_interfaces/action/Fibonacci"` / `""`, talker/listener
      `std_msgs::msg::Int32::TYPE_NAME`/`TYPE_HASH` →
      `"std_msgs/msg/Int32"` / `""` (matches
      `examples/qemu-arm-freertos/cpp/*` pattern). All 6 examples
      build clean under `cmake --build build-zenoh`. Landed in this
      Track B commit.
- [x] **220.B.3** Add a `phase212_n12_cpp_api_drift` lint to
      `nros-tests` that scans `examples/**/cpp/**/*.cpp` for the
      retired symbol names (`EntityKind`, `pub.id`, etc.) so future
      sweep gaps are caught at test time. Landed at
      `packages/testing/nros-tests/tests/phase212_n12_cpp_api_drift.rs`;
      scans every `examples/**/*.cpp`, ignores comments, fails the
      test with a per-line violation list. Passes today.

### C — Restore native + freertos rust cyclonedds CMake fixtures

* **Symptom**: `just native build-fixtures` invokes `cmake -S
  examples/native/rust/talker -B build-cyclonedds` but the
  CMakeLists.txt was deleted by Phase 212.M.1; same for
  `examples/qemu-arm-freertos/rust/talker` (Phase 212.M.5.b).
  Mitigated in `4be1a4a41` by an `[ -f CMakeLists.txt ] || continue`
  skip in both `just/native.just` + `just/freertos.just`, but the
  cyclonedds cmake variants of these fixtures are now uncovered.
* **Detection**: surfaced in driver8/13 with `CMake Error: source
  directory does not appear to contain CMakeLists.txt`.
* **Remedy**: either
  * (1) restore minimal CMakeLists.txt for the cyclone variants
    (sibling `examples/native/rust/talker-cyclonedds/` etc., per
    Phase 175.A pattern), OR
  * (2) declare the cyclone Rust path is pure-cargo only (K.7.7
    proved it works) + retire the cmake/corrosion cyclone wiring +
    its build-fixture invocation entirely.

- [ ] **220.C.1** Decide (1) vs (2). Path (2) is cleaner — K.7.7
      shipped pure-cargo native rust cyclonedds e2e; the cmake path
      was a Phase 175.A bridge that's now redundant.
- [ ] **220.C.2** Document the decision in book/internals/cyclonedds-
      backend.md + update the `just <plat> build-fixtures` recipes.

### D — Zephyr logging-smoke workspace-path mismatch

* **Symptom**: `just zephyr build-fixtures` invokes
  `cmake -P /home/aeon/repos/nano-ros/zephyr-workspace/zephyr/cmake/
  pristine.cmake` but `just zephyr setup` populates the workspace
  at `nano-ros-workspace/` (sibling-dir of nano-ros root).
* **Detection**: every driver run since zephyr setup completed.
* **Remedy**: change the logging-smoke recipe to derive the workspace
  path from the same env / config that `just zephyr setup` uses, OR
  symlink one path → the other.

- [x] **220.D.1** Audit `packages/testing/nros-tests/bins/logging-
      smoke-zephyr-native-sim/` build recipe for the hardcoded path
      + parameterise. (Landed on `phase-220-d-zephyr-workspace-path-
      fix`.) Root cause was twofold: (a) the `build-logging-smoke`
      recipe in `just/zephyr.just` used `cd "$WORKSPACE" && west
      build -d build-logging-smoke $NROS/...` with `$NROS=basename
      $(pwd)` (`nano-ros`), so both the build dir and SOURCE_DIR
      were resolved relative to whichever workspace `cd` landed in,
      and (b) a leftover `nano-ros-workspace/build-logging-smoke/`
      from a previous run had `ZEPHYR_BASE:PATH=/home/aeon/repos/
      nano-ros/zephyr-workspace/zephyr` baked into its `CMakeCache.
      txt`. `west build -p auto` then re-invoked the (now-missing)
      `pristine.cmake` under that legacy `nano-ros/zephyr-workspace/
      zephyr/` path — a cache that's been stale ever since the
      Phase 218 migration to the sibling `nano-ros-workspace/`
      layout. Fix: (1) resolve `WORKSPACE_ABS` + `FIXTURE_SRC` +
      `BUILD_DIR` to absolute paths up front (no more relying on
      cwd or `cd` ordering); (2) detect a stale `CMakeCache.txt`
      whose recorded `ZEPHYR_BASE` no longer exists and `rm -rf`
      the build dir before re-running west. Verified: a fresh
      `just zephyr build-logging-smoke` boots through codegen +
      `west build` + `ninja` to `[1242/1242] Running utility
      command for native_runner_executable`.
- [x] **220.D.2** Sweep for other stale `zephyr-workspace`
      references in recipes. Found one sibling — `build-c-port`
      (Phase 121.6) at `just/zephyr.just:1253` similarly
      hardcoded `zephyr-workspace/env.sh` instead of
      `{{ZEPHYR_WORKSPACE}}/env.sh`; same fix pattern applied.
      The patch scripts under `scripts/zephyr/*.sh` already carry
      `IN_TREE_WORKSPACE` fallback logic that picks the sibling
      `../nano-ros-workspace/` when the in-tree dir is absent, so
      no edits needed there. Docs under `book/src/getting-started/
      zephyr.md` + `docs/guides/zephyr-setup.md` describe the
      in-tree path as default w/ legacy sibling fallback — left
      as-is.

### E — Codegen-stamp churn from `nros ws sync` regen

* **Symptom**: `nros ws sync` is the canonical regen entry, but
  successive CLI versions emit subtly different `[patch.crates-io]`
  blocks (some include `nros` + `nros-rmw-zenoh` path-deps, some
  don't). Manual additions to those blocks survive ONLY until the
  next regen — they get blown away. During this sweep, I had to
  re-add `nros = { path = ... }` + `nros-rmw-zenoh = { path = ... }`
  to zephyr-rust patch blocks twice (driver17 + driver19) because
  ws sync wiped them.
* **Detection**: post-regen, zephyr rust examples failed with `error:
  no matching package named nros found` (no patch entry → cargo
  searches crates.io).
* **Remedy**: `nros ws sync` writer should be aware of every nros-*
  crate the example references in its `[dependencies]` + emit a
  path-dep for each in the patch block — not just the
  `nros-core` + `nros-serdes` minimal set today.

- [ ] **220.E.1** Audit `nros-cli/packages/nros-cli-core/src/cmd/
      ws.rs::run_sync` patch-block writer.
- [ ] **220.E.2** Extend the writer to scan the example's
      `[dependencies]` for every `nros-*` crate that has `version =
      "*"` and emit a path-dep for it.
- [ ] **220.E.3** Add a fixture in `examples/templates/local-msg-
      package/` covering an example with `nros-rmw-zenoh = "*"`
      (registry-style) — currently every fixture uses path-deps so
      this regression class isn't exercised by CI.

### F — `nros_node_options_t` typedef collision (CLOSED)

* **Symptom**: hand-written `node_pkg.h` defined
  `typedef struct nros_node_options_t { name; namespace_; domain_id; }`
  which collided with cbindgen-emitted `nros_generated.h`'s
  `nros_node_options_t` (different fields — buffer-baked Phase 88+
  shape).
* **Fix**: renamed to `nros_node_pkg_options_t` (parallel commit on
  origin/main converged with my `nros_decl_node_options_t` attempt;
  origin's name won in rebase). All 12 nuttx + freertos C example src
  files updated to the new name.

- [x] **220.F.1** Rename landed (origin commit + `4be1a4a41`).

### G — `nano_ros_node_register` doesn't link interface lib

* **Symptom**: `nano_ros_node_register(... SOURCES src/X.c
  ${pkg_GENERATED_SOURCES})` defines a static lib target but doesn't
  link `<pkg>__nano_ros_{c,cpp}` (the interface lib that owns the
  generated headers' include dirs). User code `#include "<pkg>.h"`
  fails: `No such file or directory`.
* **Fix**: appended `target_link_libraries(<component>
  PUBLIC <pkg>__nano_ros_{c,cpp})` to 18 example CMakeLists (6 nuttx
  C + 6 freertos C + 6 threadx-linux cpp). Mechanical, repeated per
  example.
* **Followup**: extend `nano_ros_node_register` to auto-link the
  matching interface lib when it sees a `*_GENERATED_SOURCES` token
  in SOURCES. Eliminates the per-example boilerplate.

- [x] **220.G.1** Per-example link appended (`4be1a4a41`).
- [x] **220.G.2** `nano_ros_node_register` now auto-links via a
      DIRECTORY-scoped registry: `nros_generate_interfaces` (in
      `cmake/NanoRosGenerateInterfaces.cmake`) appends each created
      `<pkg>__nano_ros_{c,cpp}` to the directory property
      `NROS_GENERATED_INTERFACE_LIBS`; `nano_ros_node_register` (in
      `cmake/NanoRosNodeRegister.cmake`) reads that property after
      creating the component STATIC lib and runs
      `target_link_libraries(<component> PUBLIC ${libs})` (de-duped).
      DIRECTORY (not GLOBAL) scope so multi-pkg workspaces don't
      cross-pollinate one pkg's libs into another pkg's component.
      The 220.G.1 per-example appendix lines were reverted from all
      18 CMakeLists. Smoke-verified by configure + first-compile pass
      on `examples/qemu-arm-nuttx/c/talker`,
      `examples/qemu-arm-freertos/c/talker`, and a standalone fixture
      exercising the property → `LINK_LIBRARIES` propagation; the
      threadx-linux cpp configure now hits an unrelated
      `nros_threadx_codegen_system` orchestration error
      (`play_launch_parser` missing) AFTER `nano_ros_node_register`
      ran cleanly with the auto-linked interface lib.

### H — Post-218 install-nros.sh + ~/.nros/bin sweep

* **Phase 218 install recap**: `scripts/install-nros.sh` deleted
  (`19d1d29ba`); new flow = `git submodule update --init
  packages/cli` + `just setup-cli` (builds in-tree) + `source
  ./activate.sh` (PATH wiring).
* **Symptom**: pre-218 docs / agent scripts / CLAUDE.md / Phase
  214.I-style workflows still reference `install-nros.sh` and/or
  `~/.nros/bin/`.
* **Remedy**: sweep + update.

- [x] **220.H.1** `rg -ln 'scripts/install-nros\.sh'` → replaced
      with `just setup-cli` / `source ./activate.sh` across active
      docs, scripts, cmake, integration shells, Rust tests, and
      example READMEs. Archived phase docs got a Post-Phase-218
      callout block at the top, preserving original references as
      historical record.
- [x] **220.H.2** `rg -ln '~/.nros/bin/nros'` → resolved:
      install-flow references (cmake, scripts, integration adapters,
      Rust test resolver) now prefer the in-tree
      `packages/cli/target/release/nros` path with `~/.nros/bin/nros`
      kept as a transitional fallback. `~/.nros/sdk/` paths for
      non-nros tools (zenohd, play_launch_parser, MicroXRCEAgent)
      were preserved per task scope.
- [x] **220.H.3** Phase 214.I superseded by Phase 218 — the in-tree
      build IS the install now; `NROS_FROM_SOURCE` is moot.
      Cross-ref 218.D (`just setup-cli` + `nros_cli_bin` resolver).
      Track A's pin-bump cadence + Phase 204's `install-nros.sh` pin
      bumps are likewise moot; callouts added to those docs.

### I — Zephyr rust cyclonedds bringup-pkg naming drift

* **Symptom** (surfaced 2026-06-04 by 220.D agent verification):
  `just zephyr build-fixtures` zephyr/rust/cyclonedds variants hit
  `nros codegen-system failed (rc=1)`:
  ```
  directory ".../examples/zephyr/rust/action-client" does not match
  any bringup package in workspace; known bringup pkgs:
  ["nros_zephyr_action_client"]
  ```
  Codegen-system maps the example dir → bringup pkg name but the
  drift is: example dir is `action-client`, expected bringup pkg
  is `nros_zephyr_action_client`. The naming convention bridge is
  missing on zephyr rust path.
* **Detection**: `just zephyr build-fixtures` filter
  `NROS_ZEPHYR_FIXTURE_FILTER=rust-action-client-cyclonedds`.
* **Remedy**: either
  * (1) rename example dirs to match bringup pkg names
    (`action-client` → `nros_zephyr_action_client/` etc.), OR
  * (2) extend `nros codegen-system` dir → pkg resolver to accept
    multiple aliases per workspace pkg, OR
  * (3) emit per-platform `pkg-name → src-dir` map in workspace
    metadata so codegen doesn't depend on dir-name convention.
  Path (3) is cleanest — keeps example dirs readable.

- [ ] **220.I.1** Audit `packages/cli/nros-cli-core/src/cmd/
      codegen_system.rs` (or equivalent) for the dir → bringup-pkg
      resolver.
- [ ] **220.I.2** Pick path 1/2/3; implement.
- [ ] **220.I.3** Verify all zephyr rust cyclonedds variants
      configure clean.

### J — play_launch_parser not on PATH in fresh worktrees

* **Symptom** (surfaced 2026-06-04 by 220.G agent):
  `threadx-linux/cpp` build hit `nros plan` failure: `nros plan`
  shells out to `play_launch_parser` but the binary isn't on PATH
  in fresh worktrees. Activate.sh wires `~/.nros/sdk/play_launch_
  parser/bin/play_launch_parser` symlink path; worktrees that don't
  source activate.sh hit the gap.
* **Remedy**: agent driver scripts MUST `source ./activate.sh`
  before any `nros` invocation (overlaps with 220.A.3). Plus:
  workspace doctor should FAIL when `play_launch_parser` is
  missing (not just warn) since the codegen-system path is now
  on the critical path.

- [x] **220.J.1** Sweep agent driver dispatch templates for
      `source ./activate.sh` (cross-ref 220.A.3). Landed in this
      Track J commit. Audit of `scripts/`, `.github/workflows/`,
      and the in-doc driver-loop template found a single non-
      compliant template: the `Discovery method` driver-loop in
      this very phase doc was using `export
      PATH="$HOME/.nros/bin:$PATH"` (pre-218 install-nros.sh path)
      which misses `play_launch_parser`. Replaced with `source
      ./activate.sh` + an inline comment explaining why. CI
      workflows (`zephyr-dual-line.yml`, `nros-acceptance.yml`)
      were already activate-sh-aware (per Phase 218.C) and pass
      through the env via their existing `source` calls; no edit
      needed.
- [x] **220.J.2** `just doctor` FAIL on missing
      `play_launch_parser` (currently warns). Landed in this
      Track J commit (`just/workspace.just::doctor`): the check
      now (a) requires both the stamp file AND the `bin/play_
      launch_parser` binary to exist, AND (b) verifies `command
      -v play_launch_parser` resolves — i.e. the install dir is
      actually on PATH. Three states emitted: `[OK]` (installed
      + on PATH), `[PATH]` (installed but PATH not wired — hint:
      `source ./activate.sh`), `[MISSING]` (not installed at all
      — hint: `just workspace install-play-launch-parser`). Both
      `[PATH]` and `[MISSING]` set `fail=1` (doctor exits 1).
- [x] **220.J.3** Document the contract in CLAUDE.md so future
      agents pick up the source-activate.sh requirement. Landed
      in this Track J commit (CLAUDE.md `## Practices`): added
      a "Sweep contract for agents" bullet stating every `just
      <plat>` invocation needs `source ./activate.sh` first
      (PATH wires `nros`, `play_launch_parser`, `zenohd`, etc.),
      with cross-refs to Phase 220.A.3 / 220.J.

---

## Acceptance

* [ ] `just doctor` clean on a fresh clone (no shadow warnings, no
      MISSING items).
* [ ] `just build-test-fixtures` BF-EXIT 0 (all 8 platforms green,
      zephyr solo green).
* [ ] `NROS_SKIP_FIXTURE_CHECK=1 just test-all` TA-EXIT 0.
* [ ] No new tracks discovered during a 2nd full driver pass.

---

## Cross-refs

* Phase 212.N.12 (C++ component sweep) — root cause of B.
* Phase 212.M.1 / M.5.b (Rust example collapse to pure-cargo
  Component pkg shape) — root cause of C.
* Phase 214.I (install-nros.sh env-var path) — superseded by Phase
  218 (CLI in monorepo).
* Phase 218.* (CLI merge into monorepo) — re-routed setup flow,
  deprecated install-nros.sh.
* Phase 214.P (NROS_APP_CONFIG board-side cmake emission for
  riscv64) — mirror pattern applied to threadx-linux in this sweep.
* Phase 175.A (cmake-bridge native rust cyclonedds path) — what
  C track is trying to either keep alive OR retire.

---

## Discovery method

Driver loop pattern (`driver1` through `driver20`):

```bash
nohup bash -c '
# Phase 220.J — MUST source activate.sh before any `just <plat>`
# invocation: it PATH-wires the in-tree `nros`, `play_launch_parser`
# (`nros plan` shells out to it), `zenohd`, etc. The pre-220 pattern
# `export PATH="$HOME/.nros/bin:$PATH"` misses `play_launch_parser`
# and silently hits "binary not found" inside `nros plan`.
source ./activate.sh
just build-test-fixtures > /tmp/bf<N>.log 2>&1
bf_rc=$?
echo "BF-EXIT $bf_rc"
if [ $bf_rc -eq 0 ]; then
  NROS_SKIP_FIXTURE_CHECK=1 just test-all > /tmp/test-all<N>.log 2>&1
  echo "TA-EXIT $?"
fi
' > /tmp/driver<N>.log 2>&1 &
```

Per-iteration: wait, check joblog (`tmp/build-test-fixtures-latest/
build-test-fixtures.joblog`), tail bf<N>.log for first error, patch,
re-fire. Average ~5-25 min per iteration depending on which platform
hit the failure.

20 iterations to reach a state where 4/5 pool platforms (native,
qemu, nuttx, freertos) PASS consistently. threadx_linux + zephyr
remain RED. Track-A/B/D unblocks needed before the sweep can finish.
