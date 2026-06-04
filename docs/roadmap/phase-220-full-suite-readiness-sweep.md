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

- [ ] **220.B.1** Inventory the post-N.12 C++ entity-descriptor +
      kind-enum names by grep'ing
      `packages/core/nros-cpp/include/`.
- [ ] **220.B.2** Sweep `examples/threadx-linux/cpp/*/src/*.cpp` (6
      files) to the post-N.12 API.
- [ ] **220.B.3** Add a `phase212_n12_cpp_api_drift` lint to
      `nros-tests` that scans `examples/**/cpp/**/*.cpp` for the
      retired symbol names (`EntityKind`, `pub.id`, etc.) so future
      sweep gaps are caught at test time.

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

- [ ] **220.D.1** Audit `packages/testing/nros-tests/bins/logging-
      smoke-zephyr-native-sim/` build recipe for the hardcoded path
      + parameterise.
- [ ] **220.D.2** Sweep `find . -name '*.cmake' -exec grep -l
      'zephyr-workspace' {} +` for other instances of the stale path.

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
- [ ] **220.G.2** Make `nano_ros_node_register` auto-link the
      interface lib so future examples don't need the manual
      `target_link_libraries`.

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
export PATH="$HOME/.nros/bin:$PATH"
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
