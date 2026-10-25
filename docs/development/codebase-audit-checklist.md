# Codebase Audit Checklist (tech-debt / antipatterns / UX / gate reach)

Reusable checklist for a periodic sweep of nano-ros for **tech debt**,
**antipatterns**, and **developer/user UX issues**. Re-run it each cycle and
diff against the last findings.

Prior one-off audits this supersedes/absorbs: the 214 antipattern audit
(archived), 221 build/test scan, RFC-0038 portability audit, issue 0049
(example-source cleanliness), issue 0050 / phase-247 (weak-symbol gate),
`crates-io-metadata-audit.md`.

## How to run

- **Quick triage** — grep-led, a handful of parallel readers, produce a flat
  findings list (item → file:line → 1-line). Good for a fast "what regressed".
- **Deep audit** — one agent per category below, each emitting findings with
  `file:line`, severity, and a fix sketch; then an adversarial verify pass
  (drop false positives) and a ranked report. Best run as a fan-out workflow.
- **Gate-reach audit** — category **K** alone, by mutation: plant the defect a
  gate names and require it to go red. This is the method of
  [phase-472](../roadmap/archived/phase-472-gate-reach-sweep.md) and of the
  2026-09-28, 2026-10-01 and 2026-10-07 findings logs; re-run it whenever a
  class fix lands.

**A gate beats a grep.** Where an item names a gate (`just check <name>`), the
class is enforced: do not re-run the item's grep and re-judge its hits — ask
the K question of the gate instead (can it fail on the case it names, and does
a lane run it?). A grep is for the classes no gate covers yet.

Each finding: `category · file:line · severity(P1–P3) · one-line · fix sketch`.
File new durable items into the right series (issue / RFC / phase) and link
them back here. **Detection hints are starting points, not proof — every hit
needs a human/agent read to confirm it's real (the grep is the net, not the
verdict).**

## Severity

- **P1** — correctness/safety risk, or design-violating in a way that will
  bite (silent wrong behavior, broken SSoT, unsound `unsafe`).
- **P2** — real debt that raises change-cost or drift risk (duplication,
  fragile coupling, non-self-contained packages).
- **P3** — hygiene/polish (naming, dead code, doc staleness).

---

## A. Build system / CMake

- **A1 Copy-paste drift.** Same logic in >1 cmake file diverging over time
  (the phase-246 generator class). Sweep `cmake/`, `cmake/platform/*.cmake`,
  `zephyr/cmake/`, board `board.cmake`, `NanoRosLink.cmake`, `NanoRosEntry.cmake`.
- **A2 `STREQUAL ""` on maybe-unset vars.** An omitted `cmake_parse_arguments`
  one-value keyword leaves the var UNSET; `X STREQUAL ""` then compares the
  literal name → branch fires wrong (cf. 246.3). Prefer `if(X)` truthiness.
  Detect: `grep -rn 'STREQUAL ""' cmake/ zephyr/cmake/`.
- **A3 CMake version-floor / genex availability.** `$<LINK_LIBRARY:…>` (3.24)
  guarded for the 3.22 floor; `cmake_minimum_required` consistent.
- **A4 Absolute paths / walk-up to repo root.** See **G2** (self-containment) —
  cmake half. `file(RELATIVE_PATH)` for emitted paths (214.B).
- **A5 Cache-var collisions / global-cache pollution; scattered `find_program`.**

## B. Rust / C / C++ code

- **B1 `unsafe` discipline.** Every `unsafe {}` has a `// SAFETY:` rationale;
  edition-2024 forms (`unsafe extern`, `#[unsafe(no_mangle)]`). Detect:
  `grep -rn 'unsafe' --include=*.rs` then read.
- **B2 FFI boundary.** `#[repr(C)]` on all crossing types; cbindgen config
  current; no duplicate `#[no_mangle]` symbols (the E0428 class).
- **B3 `unwrap()`/`expect()`/`panic!` in non-test runtime** on embedded paths.
- **B4 Dead code.** `#[allow(dead_code)]` sprawl, `_unused` without comment.
- **B5 no_std cleanliness — Rust AND C/C++.** Rust: code pulling `std` (or
  `alloc` needlessly) that could be `no_std`/stack-only on an embedded path;
  `#![no_std]` presence + `std` feature gating; embedded crates must compile
  `no_std`; `std` is being DELETED from the core crates (ARCHITECTURE §2,
  phase-359), so new `std::` there is a finding. Detect: `grep -rn 'use std::'
  packages/` in crates meant to be `no_std`; `std::`-qualified stdio in a
  `#![no_std]` crate is gated (`just check no-std-stdio`, issue 0589).
  C/C++: headers stay freestanding-compatible — no heap/libc calls on paths
  embedded builds compile, no exceptions/RTTI, and a hosted-only include
  (`<string>`, `<vector>`, …) needs BOTH probes, in the one spelling
  `#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__
  && __has_include(<hdr>))` — `__STDC_HOSTED__` alone misses `-nostdinc++`
  (0112), `__has_include` alone opens a freestanding libstdc++'s `#error`
  (1240), and `NROS_CPP_STD` alone removes the surface everywhere, because
  nothing that ships defines it. Gated: `just check cpp-freestanding-includes`.
- **B6 Magic numbers.** Hardcoded sizes/caps/timeouts (stack sizes, buffer
  lengths like `char buf[128]`, retry counts, ms delays, port numbers) instead
  of a named const / Kconfig / board-metadata / config knob. **No magic
  numbers** — every literal with semantic meaning is named + sourced. Detect:
  `grep -rnE '\[[0-9]{2,}\]|= ?[0-9]{3,}' packages/` then read.
- **B7 Non-gated debug messages.** `printf`/`eprintln!`/`LOG_*`/`println!`/
  `dbg!` that fire unconditionally instead of behind a log-level / Kconfig /
  `log` macro / debug feature. Noise on the hot path + binary bloat. Detect:
  `grep -rnE 'eprintln!|println!|dbg!|printf\(|fprintf\(stderr' packages/ |
  grep -v test` and check each is level-gated.

## C. API design & layering

- **C1 C/C++ user API = THIN Rust wrappers.** The public C and C++ user-facing
  API must be a thin shim over the Rust core — NO business logic, state
  machines, or duplicated behavior in the C/C++ layer; it forwards to Rust
  (CFFI) and only adapts types/ergonomics. Flag any C/C++ that reimplements
  what Rust already does, or holds logic that belongs in the core. Check
  `packages/api/nros-cpp/include/`, `nros-c/`, the CFFI seam.
- **C2 Layer-map conformance (RFC-0001).** Deps flow the right direction;
  no lower layer reaching up; `packages/drivers/` category split (RFC-0012).
- **C3 Generated-vs-handwritten boundary.** No hand edits to
  `*/generated/`; messages only via codegen (CLAUDE.md).
- **C4 Configuration-hierarchy conformance (RFC-0049).** Platform/board/app
  configuration resolves through the RFC-0049 hierarchy — no config fact
  living at the wrong layer (board data hardcoded in platform code, app knobs
  baked into board files), no bypass of the resolution order, no new ad-hoc
  config channel beside it. Check `cmake/board/`, `packages/boards/*/config/`,
  `config/`, per-example `config.toml` handling.
- **C5 Axis-agnostic core + C-ABI bridges (cargo-feature discipline).** The
  RMW and platform axes are selected via env → cargo-feature / `-D` lowering
  (one consistent mechanism across Rust/C/C++ — RFC-0031/ARCHITECTURE §2),
  and both axes cross into the core ONLY through their stable C-ABI vtable
  bridges (`nros-rmw-cffi` at `packages/rmw/cffi/`, `nros_platform_*`). Flag: core crates (`nros`,
  `nros-node`, `nros-c`, `nros-cpp`, `nros-core`) carrying
  backend/platform-specific code or `cfg(feature = "rmw-…"/"platform-…")`
  branches beyond the sanctioned registration seams; low-level backend types
  or boilerplate leaking through the bridge into agnostic code; a new knob
  selected any way other than the env/feature lowering. Detect:
  `rg -n 'cfg\(feature = "(rmw|platform)-' packages/core/` then judge each
  hit against the registration-seam allowlist; `rg -n 'zenoh|cyclonedds|xrce'
  packages/core/nros-node/src packages/api/nros/src` (names of concrete
  backends appearing in agnostic layers).
- **C6 User-API shape follows standard ROS conventions.** The C, C++, and
  Rust user surfaces mirror rclc / rclcpp / rclrs respectively (RFC-0018/0019
  + RFC-0044's rclcpp-faithful component model), and the CMake surface
  mirrors ament (`find_package(nano_ros)`, `nano_ros_add_node` ~
  `rosidl_generate_interfaces`-era shapes — RFC-0048). Flag: an entity/verb
  whose NAME or call shape diverges from the standard counterpart without
  being a documented nano-ros ADDITION (additions are fine; renames or
  reshapes of standard concepts are drift); signature drift between the three
  language surfaces for the same concept; cmake verbs that require
  nano-ros-specific ceremony where the ament shape exists. Sample:
  `packages/api/nros-cpp/include/rclcpp*/` vs the rclcpp class list,
  `packages/api/nros-c/include/{rcl,rclc,rcl_action,rclc_lifecycle,…}/` vs rclc
  (the C user surface is rclc-only since phase-501; `nros/*.h` is internal),
  `nros`/`nros-node` pub API vs rclrs, `cmake/NanoRosVerbs.cmake` vs ament verbs.
  The recorded differences have their own review in
  [phase-515](../roadmap/phase-515-api-stock-shape-review.md); start there.
- **C7 RMW vtable API mirrors rmw.h.** `nros-rmw` / `nros-rmw-cffi`'s vtable
  is our analogue of the official `rmw.h` entry-point set — same concept
  coverage, naming derivable from the rmw counterpart, same ownership/
  lifecycle semantics; ours differs by being a vtable (registration +
  dispatch) rather than link-time symbols. Flag: vtable entries with no
  rmw.h counterpart and no documented rationale; rmw.h concepts we cover via
  a different-shaped seam without a note; backend-specific parameters or
  types in the vtable signature (belongs behind the backend); semantics
  drift (e.g. an entry that blocks where rmw's is non-blocking). Coverage and
  shape are gated (phase-376): `just check rmw-api-parity` (every contract
  symbol is CLASSIFIED, cross-checked against the vtable) and `just check
  rmw-abi-shape` (the vtable mirrors name, args, return). The reader judges
  what those cannot: each `gap` row's reason and tracked issue, and ownership/
  blocking semantics. Sample: `packages/core/nros-rmw/src/`,
  `packages/core/nros-rmw-abi/`.

## D. Codegen / interfaces

- **D1 Template (jinja) drift / divergence** across the codegen templates.
- **D2 RFC-0033 per-field capacity** coverage; the two generators stay in sync
  (post-246 they share the core — guard against new divergence).

## E. Testing

- **E1 No compile inside tests; env/staleness checks instead.** Fixtures are
  built BEFORE the test run (`just build-test-fixtures` / platform recipes);
  tests never `cargo`/`cmake`/`idf.py`/`west build` at RUN time — they
  resolve the prebuilt binary, run the env-prerequisite gates
  (`is_*_available` version probes are fine) and the STALENESS probe
  (`require_prebuilt_binary` + `rust-fixture-stale.sh`/
  `cmake-fixture-stale.sh` class), and error toward the build recipe when
  the artifact is missing or stale. Flag BOTH directions: a test that
  compiles, AND a fixture consumer that skips the staleness check (museum-
  binary trap, issues 0148/0164/0196/#215). Detect:
  `grep -rnE 'Command::new\("(cargo|cmake|west|idf.py)"' packages/testing/`
  (each hit must be a version probe or a documented compile-check-fixture
  cell); `rg -L 'require_prebuilt|stale' <new fixture resolvers>`.
- **E2 Pass-on-unmet-precondition.** Bare `eprintln!`+`return` reports PASS;
  must `assert!`/`bail!`/`nros_tests::unmet!` (issue 1758: a skip is only
  `lane_skip!` from the lane's scope). Gated: `just check no-vacuous-tests`
  (a body whose only effects are prints) and the skip budget (`_check-skip-budget`,
  any non-`lane` skip). Read for what both miss: an early return after a
  probe that is not a print.
- **E3 Phase-numbered test names** (`phase212_n9_…`) — forbidden; name by
  behavior (CLAUDE.md). Detect: `grep -rnE 'fn .*phase[0-9]' packages/` (0 hits
  on 2026-10-24 — record the count; any hit is new).
- **E4 Skipped / ignored / flaky.** Every `#[ignore]` names its reason and a
  tracked issue; a QEMU red is retested SOLO before it is called flaky
  (CLAUDE.md). Detect: `grep -rn '#\[ignore' packages/testing/`. (Historical
  instances: issue 0035, phase-226 — both archived.)
- **E5 Coverage matrix: RMW × language × platform.** Every supported cell of
  the three axes has a runtime test lane (or a documented carve-out — e.g.
  "nuttx cells are arm-only by design", experimental gating). Cross-read
  `examples/fixtures.toml` + the `examples/README.md` coverage matrix + the
  `rtos_e2e` / convergence-matrix parametrization against the supported-axis
  claims in ARCHITECTURE §2. Flag: a claimed-supported combination no test
  exercises; a lane that exists for one language but silently not its
  siblings on the same platform/RMW; carve-outs that live only in tribal
  memory rather than a doc/issue.

- **E6 Matrix-derived lanes (RFC-0051).** Every runtime e2e lane is a cell of
  the declared test matrix (platform × language × RMW × workload), consumed
  by a parametrized matrix file — not a hand-written per-cell test file.
  Detect: a NEW `tests/*_e2e.rs` that spawns a fixture pair without
  consuming the matrix tables — `matrix::CELLS` for baked cells,
  `interop::CELLS` for peer/bridge cells (phase-324). Coverage is gated by
  `matrix_fixture_coverage.rs` G1–G4; read for a test that hand-picks a
  fixture without a cell.
- **E7 Output-marker discipline.** Example nodes follow the stock ROS 2 demo
  behavior contract, and ALL output markers live in
  `nros-tests/src/output.rs`; tests assert via the shared checker, never an
  inline literal. Detect: `grep -rn '"Received:"\|"I heard"\|"Publishing:"'
  packages/testing/nros-tests/tests/` — every hit outside `output.rs`
  consumers is a finding (22 such literals existed at the 2026-07-17
  survey).
- **E8 Isolation discipline.** Ports and ROS domains come from the single
  allocator (`nros_tests::alloc` / the fixtures.toml derived columns) —
  deterministic, injective across the matrix, so lanes parallelize by
  construction. Detect: port literals in test bodies
  (`grep -rnE 'tcp/[0-9.]+:(7[0-9]{3}|17[0-9]{3})'
  packages/testing/nros-tests/tests/`) that hand-mirror a fixtures.toml
  bake; a new nextest serialization group whose comment does not name the
  exclusive RESOURCE it guards (license, daemon, host-load — shared ports
  are an allocator bug, not a serialization case).
- **E9 Launch convention + micro-test budget.** Runtime launches derive from
  the RTOS framework's runner metadata (Zephyr `runners.yaml`, board-crate
  runner fields; IDF `flasher_args.json` only for the dormant esp32 lane —
  RFC-0065 D3 retired the ESP-IDF road, issue 1525) with prebuilt artifacts —
  a hand-rolled emulator command outside the `qemu.rs` interpreter needs a
  sanctioned-bypass doc-comment (E1-exception pattern). Micro-test budget:
  phase/wave-named test FILES (`w1d_*`, `phase_*`) are automatic findings
  (E3 covers fn names; this covers files); record the
  `nros-tests/tests/` file count in the findings log each audit — an
  upward trend without new matrix cells is a finding (215 tracked files on
  2026-10-24).

## F. CLI / developer UX

- **F1 Error-message quality + silent drift.** `nros` failures are actionable
  (say what + how to fix), and contracts are ENFORCED not assumed (cf. the
  board zephyr-line check, issue 0054 — declared-but-unverified → deep drift).
  Hunt for "declares X but never checks the consumer matches X".
- **F2 Bootstrap / activate friction.** Sweep contract (`source ./activate.sh`
  + `just doctor`); idempotent provisioning; clear failure on missing prereq
  (never sudo — instruct).
- **F3 Bootstrap-doc drift (static).** The book's setup/prereq pages
  (bootstrap + per-platform) cross-read against reality: `activate.sh` /
  `activate.fish`, `justfile` setup recipes, `nros-sdk-index.toml`, RFC-0014
  provisioning. Every documented command/env var/package must still exist and
  match; every required step must be documented. (A REAL clean-system run of
  those steps is issue #204 — a containerized probe, out of audit scope.)

## G. Repo hygiene & self-containment

- **G1 Submodule dirty-state / stray dirs; gitignored-but-present cruft.**
  A submodule behind its pin (`just check tier-preconditions` names it), a
  vendored tree with local edits (e.g. a formatter run over
  `third-party/`), a stale gitignored root that breaks a build (issue 1366).
  Count with `git ls-files` / `git grep`, never `find` from the checkout root —
  that walks `.claude/worktrees/*` and every submodule and counts another
  repository (issue 1565; gated by `just check repo-root-walk-scope`).
- **G2 Packages are self-contained.** A package/example must NOT search up the
  tree for the nano-ros project root, NOR hardcode absolute paths. No
  `../../../..` walk-ups to find the repo, no `/home/...` or build-host
  absolute paths baked into sources/cmake/configs; resolve via the package's
  own deps / `find_package` / relative-to-self / a passed variable. Detect:
  `grep -rnE '/\.\./\.\./\.\.|/home/|/Users/|/opt/ros' packages/ examples/
  cmake/ zephyr/ --include=*.cmake --include=*.txt --include=*.rs
  --include=*.toml` then read (some walk-ups are legit-but-flagged in CLAUDE.md;
  `/opt/ros` is legitimate in the router resolver, RFC-0075). Example build
  config is generated, not committed (`just check example-cargo-dirs`).
- **G3 TODO/FIXME/HACK/XXX density + age**; open-issue backlog triage. Detect:
  `grep -rnE 'TODO|FIXME|HACK|XXX' packages/ --include=*.rs --include=*.c
  --include=*.cpp --include=*.h --include=*.hpp`.

## H. Docs / DX

- **H1 Stale / wrong docs.** Content that describes a different project or an
  obsolete design; CLAUDE.md / AGENTS.md accuracy vs reality. A doc stating a
  VERSION or a COUNT drifts (CLAUDE.md stopped stating the
  `ros-launch-manifest` tag after it drifted twice) — prefer a doc that names
  the command that answers. This checklist is in scope too: an item citing a
  path, phase or gate that moved is a finding.
- **H2 Cross-link rot.** RFC ↔ issue ↔ phase links resolve; no orphaned docs;
  archived items actually archived.
- **H3 Book coverage/staleness** vs the current CLI/API surface.

## I. Cross-cutting antipatterns

- **I1 Duplicated code / logic** beyond cmake — Rust/C/C++ helpers copy-pasted
  across crates instead of shared. (Tech debt: the "fix one, forget the other"
  class.)
- **I2 Code violating the CURRENT design.** Implementation that contradicts a
  Stable RFC / ARCHITECTURE.md / a superseded pattern still in use (e.g.
  pre-RFC-0043 callback-by-string, pre-RFC-0044 component shapes, a retired
  launch path). Flag drift between "what the RFCs say now" and "what the code
  does".
- **I3 Silent fallbacks / swallowed errors** — warn-and-continue where it
  should hard-fail; `catch`/`Result` discarded; `|| true`.
- **I4 Duplicated SSoT.** The same fact configured in N places (issue 0042:
  capability macros across ~10 sites; domain-ID; zephyr-line; board metadata).
  One source, derived everywhere else.
- **I5 Platform `#ifdef` thickets** — sprawling per-platform conditionals that
  should be a capability/trait seam.
- **I6 Class fixes that landed only at the reported site.** For each issue
  resolved since the last audit whose defect was *grep-able*, re-run the grep and
  check the siblings were swept — and that the fix added ONE shared helper rather
  than a second spelling of the same test. Confirmed instances: the sizes-header
  mirror lineage (0088→0114→0122→0123→0245→0268), #282 fixing 1 of 6 Zephyr
  unset-variable guards (→ #326), #222 fixing 4 of ~34 fixture resolvers (→ #328).
  Related second-order check: **a gate must be able to fail on the case it names**
  — audit 2026-07-28 found four whose coverage was narrower than their rule (#321,
  #328, #332, #334). Detect: for a resolved issue's characteristic grep, compare
  hit count at fix-commit vs HEAD.
  The gate half of this is category **K**.

## J. Copy-out examples

- **J1 Examples are boilerplate-free.** `examples/**` are copy-out user
  projects (RFC-0026): they must contain ONLY what a user should copy — no
  low-level platform scaffolding, macro plumbing, FFI shims, register glue,
  or build workarounds that belong in the board/platform crates, the codegen,
  or the cmake modules. If an example needs it, the framework has a gap: file
  the gap, don't bless the boilerplate. (Successor to issue 0049's
  example-source cleanliness sweep.) Detect: read each example's `src/` +
  `CMakeLists.txt` for anything a ROS 2 user wouldn't recognize from the
  rclcpp/rclpy equivalent; grep examples for `#[no_mangle]`, `extern "C"`,
  `unsafe`, `__nros_`, raw linker flags (exclude `*/generated/`). An example's
  build configuration is generated (RFC-0098 D1) — a tracked
  `examples/**/.cargo/*` is gated (`just check example-cargo-dirs`).

## K. Gate reach

A gate is a claim that a class cannot recur. Audit the claim, by mutation
(method: [phase-472](../roadmap/archived/phase-472-gate-reach-sweep.md); the
2026-09-28 / 10-01 / 10-07 findings logs are its records). Per gate: plant the
defect it names in a scratch tree, run it the way its lane does, restore, and
refuse to continue unless the tree is back at baseline. rc≠0 is CLEAN; rc=0
needs a control (the same defect where the gate demonstrably reads) before it
is a HOLE.

- **K1 Can it fail?** A gate whose population is narrower than its rule
  (issue 0196) or wider than it (issue 1452 — a false red is a hole too). Fix
  by deriving the population, never by widening a declaration.
- **K2 Does it run?** A gate no merge-gating lane reaches answers nothing
  (issue 1226): `just check default-gates-run-somewhere` covers `just check`
  names and the `ci gate` steps. A gate in an affordability tier may resolve
  only artifacts its own job builds (`check-lane-contracts`).
- **K3 Does it say NOT VERIFIED rather than OK?** A gate that cannot see its
  subject reports a skip through the `nros_check_skip` ledger (issue 1043's
  third outcome), never a pass — e.g. ancestry from a shallow clone (issue 1476).
- **K4 Classes CLAUDE.md names, and the gate for each.** Re-read the gate's
  reach whenever one of these recurs:
  `set -e` swallowing a status at an assignment (`set-e-bare-assignment`,
  issue 1249) or in argument position (`check-workflow-indexed-apt`, issue
  1466); a git-hook script leaking `GIT_DIR` (`hook-repo-side-effects`, issues
  0986/1465); `rerun-if-env-changed` on a PATH-valued variable
  (`path-env-fingerprints`, issue 0491); a configure-time emitter with no
  reconfigure edge on its tool (`codegen-tool-reconfigure`, issue 1018); a
  propagated usage requirement conditioned on the build tree
  (`check-runtime-umbrella-link-sites`, issue 1467).


---

## Findings log

Record each run's output as `docs/development/audit-findings-<YYYY-MM-DD>.md`
(or fold into the relevant issue), so successive audits diff cleanly. Link
confirmed items to their filed issue/RFC/phase.
