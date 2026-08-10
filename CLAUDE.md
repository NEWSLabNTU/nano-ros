# nano-ros

Lightweight ROS 2 client for embedded RTOS (Zephyr, FreeRTOS, NuttX, ThreadX). `no_std`.

This file is a **router + agent practices + pitfall index**, kept short because it is loaded
every session. Design rationale lives in RFCs, operational detail in `AGENTS.md` and `docs/`.

**Docs convention — three numbered series, do not mix them:**
- **Design decision** → an RFC in [`docs/design/`](docs/design/README.md) (`NNNN-slug.md`,
  living docs; `Draft`→`Stable`→`Superseded`). Whole-system view = `ARCHITECTURE.md`.
- **Planned / in-flight work** → a phase doc in [`docs/roadmap/`](docs/roadmap/) (work items +
  acceptance; names the RFC it implements; completed → `archived/`).
- **Known bug / limitation / tech-debt** → an issue in [`docs/issues/`](docs/issues/README.md)
  (`NNNN-slug.md` + frontmatter; `status: open`→`resolved`/`wontfix`; resolved → `archived/`).
  Issues cross-link the RFCs/phases that inform or close them.

**When you learn something durable, file it in the right series above and add only a one-line
pointer here — never grow CLAUDE.md with design/impl detail.**

## Where things live

| You need… | Go to |
| --- | --- |
| Finalized whole-system design | [docs/design/ARCHITECTURE.md](docs/design/ARCHITECTURE.md) |
| A specific design decision (stable vs evolving) | [docs/design/](docs/design/README.md) — numbered RFCs |
| A known bug / limitation / tech-debt (troubleshooting) | [docs/issues/](docs/issues/README.md) — numbered issues (open) + `archived/` |
| Build / test / SDK tiers / jobserver / zephyr versions | [AGENTS.md](AGENTS.md) + [docs/development/](docs/development/) + `just/*.just` |
| Long-form practices + pitfalls (cmake, tests, multi-session) | AGENTS.md “Practices & Pitfalls” (this file keeps the one-liners) |
| `nros setup` / provisioning / `nros-sdk-index.toml` | RFC-0014 + AGENTS.md “Toolchain & SDK Provisioning” |
| ROS 2 on a host with no apt ROS (Arch, Fedora, NixOS) | [docs/development/ros2-on-non-ubuntu.md](docs/development/ros2-on-non-ubuntu.md) — Ubuntu distrobox; `scripts/dev/ros2-{distrobox-setup,box-env}.sh` |
| Feature axes (RMW × platform × ROS edition) | ARCHITECTURE §2 + RFC-0005, RFC-0006 |
| Platform/RMW impl notes + deep pitfalls | [docs/reference/platform-implementation-notes.md](docs/reference/platform-implementation-notes.md) |
| C/C++ integration shape | AGENTS.md “C/C++ Integration” + RFC-0018/0019 + [docs/reference/c-api-cmake.md](docs/reference/c-api-cmake.md) |
| User-facing workflow | [book/src/](book/src/) (`just book`) |
| Phase history / current work items | [docs/roadmap/](docs/roadmap/) (active) + `archived/` |
| Periodic tech-debt / antipattern / UX audit | [docs/development/codebase-audit-checklist.md](docs/development/codebase-audit-checklist.md) |
| Profile a build's time (passive, read-only) | `just profile <dir>` → `nros-build-profile` (phase-251); [book](book/src/user-guide/build-profiling.md) |
| Verify the book's setup flow on a pristine host | `just probe bootstrap` — runs the `probe=NN`-tagged book blocks in a clean container (`scripts/probe/`, issue 0204) |

## Naming
- **nano-ros** — project name (prose, docs)
- **nros** — code shorthand (crates, Rust/C idents, `CONFIG_NROS_*`)
- **nano_ros** — C header dir, CMake targets (`NanoRos::NanoRos`), CMake fn (`nros_generate_interfaces()`)

Workspace: `packages/{core,zpico,xrce,dds,boards,drivers,interfaces,testing,verification,reference,codegen,cli}/`,
`examples/`, `third-party/` (gitignored SDKs), `zephyr/` module. Run `ls packages/` for the current
crate list. Layer map → RFC-0001. `packages/drivers/` is split by what a crate talks
to — `net/` `serial/` `ipc/` `sys/` — documented in `packages/drivers/README.md`
(phase-321 W2.f). RFC-0012 is board/BSP integration and defines no such split.

## Practices
- **Run the TIER your change earns, after every task** (RFC-0061 / phase-318).
  **Never `sudo`** — tell the user.
  - `just ci` — **tier 1**, minutes, host only. The default. Gates and runs only
    native fixtures, so a stale ThreadX fixture cannot block it.
  - `just ci-matrix` — **tier 2**, when the diff touches `packages/core`, codegen,
    or `cmake/`. 1-wise over platform/lang/rmw/kind: every value once, ~28 % of
    the coordinates. It sees each platform and each language, but NOT their
    pairing. `just build-test-fixtures lane=tier2` is the build it needs
    (phase-340 W3): the RUN narrows to the same coordinates at fixture
    RESOLUTION time, so an out-of-lane fixture SKIPS rather than failing. Between
    #482 and W3 this lane required `lane=all` — the ~26 % was the FRESHNESS gate,
    not the build.
  - `just ci-matrix-nightly` — the pairwise cover (~70 %). Where the
    platform×language and rmw×language classes actually surface (0268/0245 sizes
    headers, 0332 freestanding headers, 0331 vtable ABI). Tier 2 costs a day of
    latency on those, which is the price of a middle tier anyone can afford.
  - `just ci-full` — **tier 3**, the whole matrix. Pre-release, on demand.
  Green tier 1 means "the logic and the seams are sound", never "it builds on the
  targets". Say which tier you ran; do not report a tier-1 green as if it were a
  sweep. The old single `just ci` WAS tier 3 — an instruction nobody could afford
  per task, so it got followed selectively, which is worse than a smaller
  instruction followed honestly.
- **Fix the CLASS, not the reported site — then prove the sweep.** Every bug here that
  recurred did so because a fix landed only where the symptom was seen: the sizes-header
  mirror (0088→0114→0122→0123→0245→0268), the Zephyr unset-variable guard (#282 fixed 1
  of 6 sites — and added a *second* idiom instead of a shared helper → #326), the fixture
  freshness probe (#222 fixed 4 RTOS resolvers, left ~30 in `binaries/mod.rs` → #328).
  So: grep for every sibling of the pattern, fix them together, add ONE shared helper
  rather than a second spelling, and put the sweep command in the commit message so the
  next person can re-run it. If a gate exists for the class, check the gate actually
  covers the new site (issue-0196 rule) — audit 2026-07-28 found four gates whose
  coverage was narrower than the rule they enforce.
- **Green CI locally BEFORE pushing — don't iterate on remote CI.** Run `just format`
  then the tier your change earns (above) locally and fix every failure first, so the
  push passes remote CI on the first try. `just ci-full` = `check` (fast + build, incl. embedded
  clippy + every per-feature/per-example clippy, and the per-component lanes `check-c` /
  `check-cpp` / `check-rmw-cyclonedds` / `check-cli-tests`) + `rust-rtos-link-check` +
  `test-all`. A backend's own test suite belongs in a `check-*` lane, never as a named step
  on the `ci` line — the Cyclone suite had one, and a red sat on main for two days because
  `just check` never ran it (issue 0319). Note: `check` runs clippy with `-D warnings`, so a toolchain bump can
  surface NEW pre-existing lints (e.g. rust-1.96 `unnecessary_cast` / `drop_non_drop` /
  `not_unsafe_ptr_arg_deref`); fix them locally rather than discovering them remotely. CI
  stops at the first failing step, so one fix can unmask the next — re-run until fully green.
- **`just format` before broad changes** (Rust + C/C++ + Python).
- **Always nightly for `rustfmt` / `cargo fmt`** — `rustfmt.toml` enables nightly-only options;
  stable produces different output. Run `cargo +nightly fmt`.
- **C/C++ style:** `.clang-format` LLVM-based, 4-space indent, 100-col.
- **Linear history:** `git pull --rebase` or `git fetch` + `git rebase`. Never merge unless asked.
- **Never `git add -A` / `git add .`** — stage the paths you actually changed
  (`git add <path>…`, or `git add -u <dir>` for tracked-only edits). A blanket add
  scoops up build output, leftover dirs and stray artifacts. Twice in one session it
  re-added a submodule dir that upstream had MOVED, as an embedded git repo — a
  gitlink with no `.gitmodules` entry, which clones as an empty directory nobody can
  populate. git prints a warning; a blanket add buries it in noise. Read `git status`
  before staging, and when a warning does appear, stop rather than push.
- **Submodule rebase on superproject pull:** if a pull advances a submodule pointer AND local work
  exists in the submodule → enter it, fetch, rebase local onto upstream, check out the
  superproject’s expected commit, record the result in the parent. Never leave a submodule at an
  older local commit when the remote pointer advanced.
- **Vendored-fork branch workflow (cyclonedds, netxduo, …):** land fixes with linear history
  (commit in submodule → `git fetch origin` + `git remote prune origin` → `git rebase origin/<branch>`
  → push). **Push the fork branch FIRST, then bump the superproject pointer** to the pushed commit.
  **By default the agent does NOT push fork remotes** (they sit outside the trusted repo →
  exfiltration guard): the agent commits + rebases locally and leaves the branch ready; the
  maintainer pushes. The agent may push only when a scoped `Bash(git -C <submodule-path> push:*)`
  allow-rule exists — never a blanket `git push:*`.
- **Codegen + orchestration CLI lives in-tree at `packages/cli/`** (a sub-workspace, own
  `Cargo.toml`/`Cargo.lock`). Edits to codegen / `colcon_nano_ros` / orchestration land there; build
  via `just setup-cli`. The retired `packages/codegen` submodule is fully gone (no stray leftover).
  `packages/cli/` nests `third-party/play_launch` + `testing_workspaces/ros2_rust_examples`.
- **Launch toolchain (RFC-0060, amended to TWO repositories — phase-332 W1/W2 landed):** nano-ros
  pins the **`play_launch`** repo at `packages/cli/third-party/play_launch`; layer 2 (the resolver,
  launch tree → SystemModel, needs CPython NOT ROS/colcon) is REGULAR FILES at
  `src/ros-launch-resolve`. Init NON-recursively (`git submodule update --init
  packages/cli/third-party/play_launch`) — layer-3 runtime submodules (`src/vendor/*`, container,
  msgs) are never built by nano-ros. `ros-launch-manifest` (spec) is a git-TAG cargo dep (`v0.1.0`),
  no longer nested — ONE copy of the spec (the 0285 double-vendoring is gone), and the old
  `--recursive` landmine is retired. The `nros-launch-resolve` helper is built by
  `just setup-launch-resolve` and invoked by ABSOLUTE PATH, never `$PATH` (issue 0285).
- **Don’t modify vendored/generated:** `third-party/`, `packages/interfaces/*/generated/`, build
  output — unless the task explicitly requires regeneration. Preserve worktree changes.
- **Examples are standalone copy-out projects** (`examples/<plat>/<lang>/<example>/`); no workspace
  walk-up. Non-example bins live under `packages/testing/{nros-tests/bins,nros-bench,nros-smoke}/`.
  Detail → RFC-0026 + `examples/README.md` coverage matrix.
- **Workspace examples follow RFC-0066 (phase-331, landed): a FEATURE is a node package,
  a CONFIGURATION is a fixture axis — never a new directory.** Feature demos (params/lifecycle/
  qos/custom-msg/remap) live as node pkgs in the native-only `workspaces/features/`; RMW ×
  feature-set variants are `fixtures.toml` rows over the four large workspaces
  (`workspaces/{rust,c,cpp,mixed}`). The themed `ws-*` dirs are GONE (W3, verified zero remain) —
  don't reintroduce one. Naming rules (no language prefixes in single-language workspaces,
  role-not-payload pkg names, one platform vocabulary for entries) →
  `examples/workspaces/README-layout.md`. West-built zephyr entry leaves need BOTH the nested
  workspace `exclude` AND a repo-root `Cargo.toml` exclude, and their dep keys must match the
  generated `<entry>_nros_selection` package name (phase-331 fallout class, 2026-08-03).
- **SystemModels are BUILD ARTIFACTS — never committed, never referenced by entries**
  (phase-330 W4.a/W7, landed 2026-08-03). Dims/params/capabilities are authored in
  `system.toml` (+ launch files); `nros sync` resolves into `<ws>/build/nros/models/
  <bringup>/`; entries name their INPUT (`nros::main!(launch = "bringup[:file]")`,
  `nano_ros_entry(BRINGUP … LAUNCH …)`); consumers locate the artifact via
  `nros_orchestration_ir::model_location` (never a hand-derived path). `model =`/`MODEL`
  are deprecated expert overrides. Gate: `check-no-tracked-models` (issue 0380 was four
  hand-edit deletions; the ban is the structural fix). Inspect with `nros ws model-dims`.
- **Message deps are PATH deps pinned `0.0.0` (RFC-0067 / phase-333)** — never registry-name a
  message crate (`std_msgs = "*"`) in a leaf manifest; #378 showed a bare name resolving against
  the PUBLIC crates.io.
- **`generated/` in examples/fixtures/tests is USER-side — never commit it, and therefore never
  commit their `Cargo.lock`.** Those trees are codegen'd from the USER's own msg packages, so they
  don't exist in a fresh clone; a lock committed beside one names crates nobody has and every cargo
  command in that leaf fails (this made `build-test-fixtures` unrunnable on a fresh host,
  2026-08-03 — ten such locks deleted). Tell users to run `nros sync`. When a lock and a missing
  `generated/` collide, DELETE THE LOCK — never commit a `generated/` tree to keep one.
  **Exception: the core pre-generated msg packages** (`packages/interfaces/*`), committed under
  `nros-`prefixed names because core crates need those messages BEFORE any codegen runs and the
  prefix keeps them from colliding with a user package of the same ROS name. They resolve from a
  bare clone, so their LOCKS are tracked — but their crate version is still the constant `0.0.0`
  like every generated crate, and consumers must path-dep them with NO version (pinning either
  spelling broke root-workspace resolution twice, #394).
  Invariant, enforced by `check-leaf-lockfiles`: **tracked lock ⟺ (no message deps) ∨ (committed
  `generated/`)** — boards/drivers/smoke qualify via the first arm. (The old "track all of them"
  rule predates the shim keying on TRACKED rather than ignored, #386.)
- **Messages are generated** (`nros generate-rust` from `package.xml`) — never hand-write. Detail
  → RFC-0023 + [docs/guides/message-generation.md](docs/guides/message-generation.md).
- Unused vars: `_name` + comment, or `#[allow(dead_code)]` for test struct fields.
- Reusable tests → `packages/testing/nros-tests/tests/` (Rust) or `tests/` (sh). Temp tests → Bash
  then promote. Temp files in `$project/tmp/` (gitignored), not `/tmp`; use Write/Edit not heredoc.
- **Tests must fail on unmet preconditions** (`assert!`/`bail!`/`nros_tests::skip!`). Bare
  `eprintln!`+`return` reports PASS — never. Same for runtime: panic, not silent early-return.
- **No compilation inside tests** — never `cargo`/`cmake`/`idf.py`/`west build` at run time. Compile in
  the build stage (`build-test-fixtures` + `examples/fixtures.toml`); the test consumes the prebuilt
  fixture. "Does it compile?" intent → make it a build-step fixture and assert the artifact. → AGENTS.md Testing.
- **Fixture builds are LANE-SCOPED (#393):** `just build-test-fixtures lane=<all|native|tier1|tier2|tier2-nightly>`
  narrows both the platform-family fan-out and the manifest rows; the `.fixtures-built` stamp
  records `lane=` + per-coordinate rows, and `_require-fixtures` checks COVERAGE against the run's
  lane. Build the lane you'll test — tier 1 doesn't need all 337 rows.
  **A lane answers TWO questions and they have different answers (#482):** which fixtures must be
  FRESH (its cell cover) vs which must EXIST (a property of the RUN). `nros_lane_build_lane` maps
  lane→required build and `CiLane::run_scope` declares it. Tier 1 narrows its run by NAME
  (`NROS_TEST_SCOPE`) so it needs the broader `native` build; **tier 2 / nightly narrow by
  COORDINATE in the fixture RESOLVER** (`NROS_TEST_COORDS` → `nros_tests::fixtures::lane`,
  phase-340 W3), so each is its own build lane. Name filtering cannot express tier 2 — it is
  1-wise over platform, so every platform is in it (#357/#482); the resolver attributes an
  artifact back to its manifest row via `row_artifact_root()`, the sibling of `row_coord()`, so
  build-set and run-set are ONE predicate on one coordinate file. The skip is keyed on the
  COORDINATE, never on "artifact absent": an in-lane fixture that is missing or stale still fails
  hard, and an unattributable path (zephyr west leaves, compile-check — built module-level) is
  never skipped. A row's coordinate still has exactly one computation, `row_coord()` in
  `fixtures-manifest.py` (`rmw` defaults to zenoh THERE) — `matrix_fixture_coverage.rs` consumes
  its `coords` subcommand rather than re-deriving. The second derivation left 67 of 240 rows in
  no lane at all. New runtime tests join a
  matrix: cells in `matrix::CELLS` / `interop::CELLS` (RFC-0051; phase-331 W4 put workspace RMW
  cells there too), not new hand-coordinated files — the consolidation plan is phase-329.
- **Fixture mtime treadmill:** any pull/rebase — and any `git stash push`/`pop`, which rewrites
  tracked files just the same — refreshes source mtimes → EVERY prebuilt fixture
  reads STALE. **A refresh re-arms the in-tree CLI's source stamp too, not just fixtures
  (issue 0466)** — and the order is load-bearing: rebuild the CLI FIRST (`just setup-cli`),
  THEN fixtures, because fixtures key on that stamp; doing it the other way re-stales
  everything you just built. `just check-tier-preconditions` reports every unmet
  precondition at once (CLI, leaf `nros sync`, build sources, fixtures for the lane)
  instead of one per attempt; it runs at the head of `just ci`. Rebase once → rebuild affected fixtures → test WITHOUT pulling again. Core-crate
  or repr(C)-struct changes ⇒ wipe workspace build dirs (incremental mixes pre/post-append
  objects → garbage-pointer SEGVs). Long-unrebuilt families "pass" on museum binaries — trust
  only a fresh full sweep, and re-measure any perf number on cleanly rebuilt fixtures before
  filing an issue from it (→ archived issues 0148/0164). A `nros` CLI rebuild also stales every
  WORKSPACE fixture (the codegen tool is in the input signature + CONFIGURE_DEPENDS since #182 —
  rebuild the family, don't debug the "runtime bug").
- **A STALE verdict is ABSORBING — read the `probe:` and NOT RUN lines before believing it**
  (issue 0445). The fixture never launches, so whatever it would have done at runtime is
  replaced by a message that explains itself; issue 0444 hid behind 0442 for exactly that
  long, and my first explanation of the symptom was plausible and wrong. Verdicts now print
  what the probe examined and exempted, and count consecutive non-running resolutions —
  `x2+` means suspect the probe, not just the fixture. `just fixture-staleness` lists every
  coordinate producing no runtime result. Exemptions have ONE spelling
  (`nros_tests::fixtures::staleness::exempt_probe_input`, gated by
  `check-staleness-probe-exemptions`) because per-arm subsets ARE 0442.
- **Test greps use `nros_tests::output::*` constants, never literal strings** — example
  banners/markers get slimmed (phase-277 broke ~10 tests grepping `"Result:"`/`"[OK]"`/old
  banners while delivery worked). If a test times out, FIRST diff the grep pattern against what
  the fixture actually prints. → archived issues 0157/0164.
- **Test names describe behavior, not phase numbers** (`zephyr_xrce_service_e2e`, not `phase212_n9_…`).
  Phases go stale; cross-ref a phase in a doc-comment, never the identifier. → AGENTS.md Testing.
- **Two test-intent lists (RFC-0051 / phase-324):** `matrix::CELLS` = baked/self-contained;
  interop & bridge cells live in `interop::CELLS` (nano `Cell` + peer + dir + build + test) —
  they have NO `fixtures.toml` row (ephemeral peer, west-leaves/native nano side). Gated by
  `matrix_fixture_coverage.rs` G1–G4; each interop test carries an `interop::assert_test_bound`
  coordinate tripwire. Don't add an interop test that hand-picks a fixture without a matching
  `interop::CELLS` row. `ros_editions_e2e` is the docker edition axis (#0327), not a cell.
- **Bare `cargo nextest` counts `nros_tests::skip!` panics as FAILURES** — only `just test-all`'s
  junit rewrite makes them skips. Read the panic text before filing a bare-run red as a regression.
  And full-sweep QEMU lanes flake under load (287-W7: six nuttx lanes failed 3/3 in-sweep, passed
  solo) — retest a QEMU red SOLO before filing. A "solo red" can ALSO be a stale-build artifact,
  not code (issue 0268: the sizes-header mirror race made incremental trees red and clean trees
  green — a bisect whose steps rebuilt clean "converged" on a docs-only commit). When a bisect's
  first-bad is implausible for the symptom, the test tracked a confounder (build state, load,
  ports) — rerun one rev N times before trusting any boundary. → AGENTS.md Test Pitfalls.
- **Build-side stale probes must watch the same inputs as test-side gates** — a probe that misses
  `generated/**` lets a museum binary pass every sweep while tests fail STALE (issue 0196).
- **Sweep contract:** every `just <plat>` invocation needs `source ./activate.sh` first (PATH wires
  `nros`, `play_launch_parser`, `zenohd`). `just doctor` enforces it. The pre-218
  `export PATH="$HOME/.nros/bin:$PATH"` is insufficient.

## Pitfall index

One-liners; detail in the linked doc. (Many also captured in agent memory.)

- **After clone, run ONE of** `direnv allow` / `source ./activate.sh` / `source ./activate.fish`
  else `zpico-sys/build.rs` panics `"FREERTOS_PORT not set"`. Activate files are the env/PATH SSoT.
- **Zenoh pinned 1.7.2** (rmw_zenoh_cpp compat). zenohd from `third-party/zenoh/zenoh/`; zenoh-pico
  from `packages/rmw/zenoh/zpico-sys/zenoh-pico/`. Tests auto-use `build/zenohd/zenohd`.
- **Rust edition 2024:** `unsafe extern "C" {}`, `#[unsafe(no_mangle)]`, explicit `unsafe {}` in
  `unsafe fn`. `nros-c` keeps `#![allow(unsafe_op_in_unsafe_fn)]`.
- **No POSIX-style Rust ctor sections on Zephyr/native_sim/RTOS** — backend registration is an
  explicit call: C/C++ via `nros_cpp_init` → strong `nros_app_register_backends`; pure-Rust via
  `zephyr_component_main!` (calls the hook + cfg-gated direct `register()`). A pure-Rust image
  needs the REAL backend dep (`rmw-zenoh = ["dep:nros-rmw-zenoh"]`) — and a direct reference,
  or rustc's staticlib DCE drops the dep's `#[no_mangle]` export (symbol in the rlib, absent
  from the `.a`; nros-c's FORCE_LINK class). → issues 0155/0163 (archived).
- **nros-cpp headers: gate `<string>`/std includes on `NROS_CPP_STD`, not `__STDC_HOSTED__`** — a
  hosted compiler can still run `-nostdinc++` against Zephyr's minimal libcpp (no `<string>`).
  → issue 0112 (archived).
- **Domain ID:** compile-time on embedded (Kconfig / per-example `config.toml`), runtime env on
  native via `nros_tests::unique_ros_domain_id()`. `CONFIG_NROS_CYCLONE_DOMAIN_ID` defaults to
  `NROS_DOMAIN_ID` — never pin it to a literal in confs (the phase-180 split-brain silently ran
  every cyclone image on domain 0). Cyclone fixture pairs bake distinct domains (50–58) for
  parallel SPDP. → issue 0161 (archived), platform-implementation-notes.md.
- **`zpico_spin_once` on multi-threaded platforms uses `z_sleep_ms()`, not `select()`** (else
  `Promise::wait()` burns its budget in ~39 ms). → platform-implementation-notes.md.
- **FreeRTOS:** `APP_TASK_STACK` 64 KB (inline executor arena on stack) → "Invalid mbox" otherwise;
  IP-seeded `srand()`; poll-task priority ≥ 4; manual action server needs
  `try_handle_get_result()`. → platform-implementation-notes.md.
- **Zephyr POSIX:** raise `CONFIG_MAX_PTHREAD_MUTEX_COUNT` (zenoh-pico needs ~8+; default 5 fails
  with -80). → platform-implementation-notes.md.
- **Zephyr zsock serializes send/recv per-fd:** `Z_CONFIG_SOCKET_TIMEOUT` must stay 100 ms (5 s
  starves tx → lease death, silent session drop); intra-image pub→sub needs
  `Z_FEATURE_LOCAL_SUBSCRIBER=1`. → platform-implementation-notes.md (issues 0129/0139).
- **NuttX spin uses `sem_timedwait`** (pthread condvar hangs). → platform-implementation-notes.md.
- **NetX Duo BSD `SO_RCVTIMEO` takes `nx_bsd_timeval*`, not `INT` ms** (deadlock otherwise).
  → platform-implementation-notes.md.
- **smoltcp multicast:** join the GROUP addr, not `0.0.0.0`; LAN9118 needs promiscuous in QEMU.
  → platform-implementation-notes.md.
- **QEMU:** `-icount shift=auto`; use `nros_tests::qemu::qemu_system_arm_cmd()`. →
  [docs/reference/qemu-icount.md](docs/reference/qemu-icount.md).
- **Embedded Cyclone:** transient samples use `ddsrt_{malloc,calloc,free}`, never libc — RTOS heap
  is separate. → [docs/reference/cyclonedds-known-limitations.md](docs/reference/cyclonedds-known-limitations.md).
- **XRCE:** flush `uxr_buffer_request_data` immediately; reliable `STREAM_HISTORY ≥ 2`.
  → platform-implementation-notes.md.
- **Zephyr Rust allocator is picolibc `malloc`** — size `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`
  (default 16 KB; executor backing alone needs ~75 KB), NOT `CONFIG_HEAP_MEM_POOL_SIZE`.
  → issue 0163 (archived).
- **A Kconfig knob reaches the Zephyr C lane and NOT the RUST one** — `nros_cargo_build.cmake`
  publishes knobs with `set(ENV{…})`, which only touches the configure-time process; the C lane
  re-bakes them into its command (`cmake -E env`), zephyr-lang-rust's `rust_cargo_application`
  builds its own and inherits nothing. So every Zephyr Rust image compiled crate DEFAULTS
  whatever Kconfig said — and when the two halves disagree it is also an 0135 ABI split
  (`MAX_QUERYABLES` 16 in the cmake TU, 8 in the cargo one). Build scripts resolve knobs with
  `nros_zephyr_build::knob_usize(env, CONFIG_key, default)` (reads `$DOTCONFIG`); gate:
  `check-kconfig-knob-forwarding`. → issue 0460.
- **A service server IS a zenoh queryable** — `[param_services]` (6) + `[lifecycle]` (5) claim
  eleven slots before the app declares anything, against `ZPICO_MAX_QUERYABLES` = 8 embedded.
  Raise `CONFIG_NROS_MAX_QUERYABLES`; the table is a static array, so the default stays small.
  → issue 0460.
- **Manual native_sim pair repros need distinct `--seed`** — unseeded processes share the test
  entropy source → identical GUIDs/ports → discovery sees the peer as itself → false-negative
  "no delivery". The test harness seeds automatically; hand-run repros must too. → issue 0157
  (archived).
- **Never clang-format `cmake/templates/*`** — reflow splits `@VAR@` configure_file tokens
  (`@SYM @_create`) → generated TU fails "stray '@'". `.clang-format-ignore` guards; format
  recipes already exclude them. → issue 0159 (archived).
- **RMW + platform C ABI: the C headers ARE the SSoT (RFC-0054)** — Rust consumes
  COMMITTED bindgen output (`packages/core/{nros-rmw-cffi,nros-platform-cffi}/src/generated.rs`).
  Header edit ⇒ run `scripts/gen-abi-bindings.sh` (pinned bindgen-cli 0.72.1) + commit both;
  `check-abi-bindings` gates staleness. Never hand-edit `generated.rs`; vtable slots are
  `Option<fn>` (C nullability); no layout tests in generated code (host-64-bit literals
  break 32-bit targets).
- **Hand-mirrored FFI structs drift on append** (QoS `tx_express`, `callback_group` — 3×):
  mirror-only TU passes a SHORTER struct by value → tail field garbage. Gated:
  `check-ffi-struct-mirrors` (push lane) + cross-include TU in `check-c`. Include order is
  one-way: `nros_cpp_ffi.h` BEFORE `component.h`. → issue 0160 (archived).
- **zpico shim + zenoh-pico library MUST share the generated zenoh config** — flag-gated struct
  fields (`Z_FEATURE_LOCAL_QUERYABLE`…) make mismatched TUs a silent ABI break (queries went
  session-local-only). `build_c_shim` injects `ZENOH_GENERIC` + the OUT_DIR config. → issue 0135
  (archived). Local fixture binaries embed the shim — rebuild fixtures after zpico config changes.
- **A lib reached through a raw `-Wl,...` link FLAG gets no rebuild edge (issue 0475)** — CMake cannot
  see a file inside a flag string, and `add_dependencies()` only adds build ORDER, which ninja renders
  `||` (order-only): "must exist before linking", never "relink when it changes". The RMW backends are
  whole-archived exactly that way, so a backend edit rebuilt the archive and left every C/C++ example
  binary holding the OLD code — museum binaries by construction, clearable only by `rm -rf` on the build
  dir (~687 s per Cyclone leaf). Fix is `LINK_DEPENDS` on the consuming target (`nano_ros_link_rmw`),
  which adds the file edge without touching the link LINE — do NOT also `target_link_libraries()` the
  archive: that reorders ld's single pass and breaks the whole-archive group (`undefined reference to
  ddsrt_*`). Verify with `ninja -C <build-dir> -t query <exe>`: the `.a` must appear under `|`, and a
  `touch` of a backend source must relink.
- **Every cargo command cmake emits passes `--target`, HOST INCLUDED (phase-340 W3)** —
  `--target <host-triple>` and no `--target` are different `-C metadata` identities that
  share nothing, not even sccache entries (measured 0 hits / 62 misses). Corrosion hardcodes
  the flag and is upstream, so it is the fixed point; resolve the triple with
  `_nros_resolve_rust_target()` (never `Rust_CARGO_TARGET` directly — it is a normal var that
  does not cross `add_subdirectory()`, which is phase-155's wrong-arch link). Gate:
  `check-cargo-target-spelling`.
- **A build with no `[[fixture]]` row has no COORDINATE, so it gets no shared cargo group
  (phase-340 P2)** — that is how a bare `cd <leaf> && cargo build` in `build-examples` kept
  re-creating `examples/**/target/` two minutes after the group dir was written, on a platform
  already migrated. Give such a build a row (preferred) or derive its dir from
  `nros_fixture_target_dir_flag` + `nros_fixture_row_artifact_dir` — **never a literal, and move
  the test-side locator in the SAME commit** (#393). `examples/**/target-*/` is globally ignored;
  a plain `target/` is not, so it is gated: `check-example-leaf-target-dirs`. A PLATFORM's fixture
  profile is `nros_cargo_platform_profile` — the staleness probe must use it too, or it rebuilds
  into a second profile dir and reports permanent false-STALE. Residue → issue 0488.
- **cmake `include()` inside a FUNCTION drops the file's normal vars when the frame pops** —
  capture module dirs `CACHE INTERNAL` (the `_NROS_ENTRY_DIR` pattern); a plain
  `set(_X_DIR ${CMAKE_CURRENT_LIST_DIR})` broke every freertos ws member's `configure_file`
  (287-W6; posix hid it). And `find_program` HINTS beat PATH — a stale `~/.nros/bin` binary
  shadows the activate.sh CLI; use `PATHS` for fallbacks. → AGENTS.md CMake Pitfalls.
- **Case-normalize enum-ish cmake args** (`string(TOUPPER)`) — the ament verbs pass lowercase
  `cpp`; a case-sensitive `STREQUAL "CPP"` silently takes the C branch. → AGENTS.md CMake Pitfalls.
- **Lockfiles change ONLY when a dev means it** (issues 0359/0378). `Cargo.lock` is a promise
  that someone else's build resolves what yours did, so `just lock-update [crate] [version] [dir]`
  is the only sanctioned way to move one — never bare `cargo generate-lockfile`, which re-resolves
  EVERY package (26 leaf locks once moved 5388 lines as a "cleanup"). `--locked` is injected
  PROJECT-WIDE by the `scripts/bin/cargo` PATH shim (`NROS_CARGO_FLAGS`, wired in `activate.sh`),
  so a mismatch FAILS instead of silently rewriting the file. Cargo has no config/env knob for it
  (`[build] locked` is an unused key), and per-site flags would miss cmake/corrosion, which invoke
  `cargo` by NAME. Escape hatch: `NROS_CARGO_FLAGS= just <recipe>`. **Generated msg crates are the
  exception**: they are produced per host by `nros sync` from the consumer's ament install and
  never shipped, so codegen emits a CONSTANT `version = "0.0.0"` (the ament version moves to
  `[package.metadata.nros] ament_version`) — otherwise a committed lock asserts which ROS install
  built it and every other host reads as drift.
- **Rust leaf `.cargo/config.toml` is `nros sync`-managed (RFC-0048 W9)**: one
  `include = ["…/nros-patch.toml"]` (central, gitignored, absolute paths) + leaf-local
  `generated/*`/platform patches. Never hand-edit; moved checkout → re-run `nros sync`. Central
  membership = only crates registry-named in EVERY graph (else cargo "unused patch" warnings).
  **Sync's `[patch.crates-io]` rows split by ORIGIN, not by "sync wrote it" (issues 0457/0463):**
  IN-REPO rows (`nros-log`, board crates, `mps2-an385-pac` — relative paths, identical in every
  checkout) stay INLINE in the tracked `config.toml`, tagged `# nros-managed`; only `generated/`
  rows go to the GITIGNORED sidecar `.cargo/nros-managed-patch.toml`, whose `include` is written
  only when that file is. So a leaf with no message dep has no sidecar, no include, and resolves in
  a fresh clone with NO sync — only ament-derived content sits behind sync. (0457 moved the WHOLE
  set to the sidecar; that stranded every leaf on `no matching package named 'mps2-an385-pac'`,
  an in-repo patch a clone needs.) The authored half (`[build] target`, a QEMU `runner`, link
  rustflags, a user `libc` patch) stays tracked because a clone cannot regenerate it. Corollary,
  gated by `check-cargo-config-tracked`: **a tracked config must never patch an uncommitted
  `generated/` tree** (`packages/interfaces/*` are exempt — they commit theirs). An out-of-tree
  consumer keeps everything INLINE: no `include` outside this checkout (#272).
  **After a sync the tracked config legitimately gains the sidecar `include` on disk — NEVER commit
  that line** (`git add -u` scoops it up; it did twice). The invariant is about the COMMITTED blob,
  so the gate reads `git show HEAD:<path>`, not the worktree. **A missing `include` target is a HARD cargo error during MANIFEST PARSE — not the
  silent drop #272 and #457 both assumed (issue 0463).** Both generated targets are gitignored, so
  before `nros sync` these leaves cannot even be READ (`cargo metadata` fails too, four frames deep,
  never naming sync). Guarded by `_require-leaf-includes`; `check-cargo-config-tracked` also rejects
  an include naming a target no generator writes. → AGENTS.md Rust Consumption.
- **Parallel agent sessions push to `main`** — **reserve issue ids with `just issue-new <slug>`,
  never by reading the highest number.** Reading-then-writing is a race that has collided seven
  times (0367→0372→0377 collided TWICE, the second time while renumbering the first). The tool
  claims `refs/issue-ids/NNNN` on origin, which git rejects if it already exists; the `pre-push`
  hook (`just setup-hooks`) refuses to push a duplicate even if the tool was skipped. Expect
  `docs/issues/README.md` rebase conflicts; write full background logs to files (`| tail` hides
  the real error). → AGENTS.md Multi-Session Pitfalls.

## Verification
Kani (bounded harnesses, `just verify-kani`) + Verus (unbounded proofs, `just verify-verus`).
Patterns + the `verify = true` footgun → [docs/guides/verus-verification.md](docs/guides/verus-verification.md).
