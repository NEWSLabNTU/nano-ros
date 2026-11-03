# Phase 530 — `nros configure`: one resolver for every road, tight sizing, and no fixed point

**Status (2026-11-03). OPEN — W0 done (fallback not taken), W1 landed (with W9 step 1); W2 next.** Implements
[RFC-0120](../design/0120-nros-configure-one-resolver.md) D1 to D7, D9 and
the driven and hooked rows of D8. Meets
[RFC-0094](../design/0094-resolve-before-configure.md) A3 as originally
written: the fixed point is deleted.

Builds on:

- [phase-439](archived/phase-439-resolve-before-configure.md) (archived), which
  landed the resolve phase, the W6 bounds seed, the W7 entity take and the W8
  promise check;
- issue 2194 (resolved), the board knob rung.

---

## Why

Phase-439 made a host configure single-pass only when `nros build` started it.
Four configures still converge through `NanoRosReconfigure.cmake`'s forged
future timestamp, the mechanism behind issues 0940, 0991, 1002 and 1119. And a
plain `west build` of a leaf is not tightly sized at all: its entity inventory
does not compose (the talker's `system.toml`, phase-481 W3), so it falls back
to crate defaults, which issue 2202 measured at ~400 KB on a 520 KiB part.

The maintainer's requirements (2026-11-03):

- `west build` and `nros build` are both tightly sized and always see the
  latest knobs;
- `nros build` is the main path and works like cargo;
- other roads are supported where a user's tool needs them;
- a host compiler is an accepted requirement.

## Work items

### W0 — Spike: the census digest as a host-configure dependency (DONE 2026-11-03)

RFC-0120 D7 depends on one behaviour. A build edge writes
`<build>/nros/census.digest` write-if-changed (`restat = 1`). That file is in
`CMAKE_CONFIGURE_DEPENDS`. Ninja should then:

- bring the edge up to date before deciding whether `build.ninja` is stale;
- re-run the host configure only when the digest's bytes changed.

Build a five-line CMake + ninja project (cmake 3.22, the tree's pin) and
measure three things:

- a source edit that leaves the digest unchanged → no `Re-running CMake`;
- an edit that changes it → exactly one;
- no "multiple rules generate" or dependency-cycle error.

Repeat on a Zephyr native_sim leaf.

- **Acceptance:** the three measurements recorded in this doc, both projects.
- List which entity-derived values are Kconfig symbols, because D7's
  build-time fallback cannot reach `.config`.
- **Decision step:** if any measurement fails, take RFC-0120 D7's fallback.
  Entity-derived knobs move to build-time delivery (a generated header, plus a
  cargo `--config` file with ninja edges), and W6 becomes that work. Record the
  choice in RFC-0120.

**Results (measured 2026-11-03).** The spike: a build edge (inputs = source)
writes `census.digest` write-if-changed, as a BYPRODUCT so ninja restats; the
digest file is in `CMAKE_CONFIGURE_DEPENDS`; configure also writes the digest
itself.

Plain CMake 3.22.1 + ninja 1.13.2 project:

| test | re-runs | configures | digest |
| --- | --- | --- | --- |
| T1 clean build | 0 | 1 | pubs=1 |
| T2 no-op | 0 | 1 | pubs=1 |
| T3 body edit (no endpoint change) | 0 | 1 | pubs=1 |
| T4 add an endpoint | 1 (`[1/2] Re-running CMake...` after the census edge ran) | 2 | pubs=2, seen by that configure |
| T5 no-op after | 0 | 2 | pubs=2 |

No "multiple rules generate", no cycle error.

Zephyr 3.7, `west build -b native_sim/native/64` on a bare hello-world app
with the same snippet (`native_sim` 32-bit needs gcc-multilib, absent on this
host; the tree uses `native_sim/native/64`):

| test | result |
| --- | --- |
| T1 clean | 0 re-runs, 1 configure |
| T2 no-op | 0 |
| T3 body edit | 0 re-runs |
| T4 add endpoint | exactly 1 re-run; the configure in that pass saw pubs=2 |
| T5 no-op after | 0 |

No errors.

**Decision: RFC-0120 D7's build-time fallback is NOT taken.** Because the
re-run is a real host configure, Kconfig is re-rendered too, so Kconfig-backed
derived values update through the same path. The Kconfig-symbol listing below
is therefore moot, and W6 stays the census-step wiring.

### W1 — `nros configure`, with sync folded in (LANDED 2026-11-03)

- A user-tier verb in the Build group (RFC-0117). It runs C1 (today's sync)
  and C2 to C6 (today's stages 1 to 4), and writes `<build>/nros/configured/`
  with a stamp holding the input digest.
- An unchanged second run is a no-op.
- `nros sync` becomes an alias: a `("sync", "configure")` row in the user-tier
  alias table (RFC-0117). With that row, `check-cli-old-spellings` (phase-526
  W4) flags every `nros sync` in the scanned tree, so run
  `python3 scripts/check/check-cli-old-spellings.py --fix` in the same change.
- `nros build` and `nros run` configure when the stamp is stale, instead of
  refusing an unsynced tree.
- The interface search path is computed inside configure. Remove
  `eval "$(nros dev ws env)"` from every book and README flow.
- The metadata probe is cached per component, keyed on the component's
  sources.
- Layout and concurrency per RFC-0120 D2:
  - stamps live at `<project>/build/nros/configured/<image>/`, plus one per
    shared host-configure coordinate;
  - a per-project file lock serialises writers;
  - the stamp records the ament prefix and the `nros-launch-resolve` path the
    run used;
  - `NROS_RESOLVED_DIR` retires in favour of locating the stamp by image.
- Measure the lock under the fixture build's parallel configures: one
  workspace reached by several configures at once.
- **Acceptance:**
  - `nros new … && nros build` succeeds with no `nros sync` typed;
  - a second `nros configure` with nothing changed exits in under one second
    (measured on the C++ workspace);
  - `nros sync` still works.

**What landed (W1).**

- **The verb.** `nros configure` is a user-tier verb in the Build group. It
  runs C1 (today's sync) and C2 to C6 (stages 1 to 4) under the project lock,
  and runs no target build tool. Flags: `--image`, `--generated-base`,
  `--ros-edition`, `--dry-run`, `--check`, `--force`, `--offline` and the
  provider and metadata flags it inherited from sync. An image the host cannot
  build is recorded as skipped and configured only when named.
- **The stamp and its staleness rules.** A stamp per image at
  `<project>/build/nros/configured/<image>/stamp.toml` holds the input digest:
  `package.xml`, `Cargo.toml`, `system.toml`, launch files, the env variables
  configure reads (`AMENT_PREFIX_PATH` among them), the `nros-launch-resolve`
  path, and the ament install's CONTENT (the digest of the value alone missed
  an in-place ROS upgrade, so a moved generated package makes the stamp stale).
  A skip stamp on a named image is stale. `--dry-run` records no stamp. A
  project with no image gets one `.project` stamp. A skip stamp re-checks its
  image's preflight. The probe cache per component and the shared host-configure
  coordinate stamps (D2) are not part of W1; see the open items below.
- **The lock.** A per-project file lock serialises writers. The parallel
  measurement the item asks for (several configures reaching one workspace at
  once, under the fixture build) is not yet run; it belongs with the fixture
  build's switch to configure.
- **Implicit configure.** `nros build` and `nros run` call
  `configure::ensure_current` before planning, like cargo fetching what it
  needs. The "has not been synced" refusal is now an internal error, since a
  configure that left the tree unsynced is a bug. `refresh_generated` stays in
  build as the ament backstop; `refresh_managed_msg_deps` is gone, covered by
  configure's C1.
- **The sweep.** `nros sync` is an alias row `("sync", "configure")`, warns on
  stderr on every use whether or not it is a terminal (W9 step 1). The tree was
  swept with `check-cli-old-spellings --fix`, so the scanned tree holds zero
  `nros sync`, and the gate keeps it so. Remedy strings in the CLI say
  `nros configure`. `eval "$(nros dev ws env)"` is gone from the book flows.
- **`--offline`.** It reaches the one step of a sync that resolves crates, the
  `cargo update --workspace` that prunes a seeded leaf `Cargo.lock`, so an
  offline configure never touches the network.

**Acceptance (measured 2026-11-03).**

- `nros new … && nros build` needs no `nros sync` typed: the build integration
  test `build_on_a_fresh_copy_needs_no_sync` passes.
- `nros sync` still works, with its warning.
- A second `nros configure` with nothing changed on `examples/workspaces/cpp`:
  **NOT MET on the C++ workspace, because the first
  configure does not finish there.** `nros configure` in
  `examples/workspaces/cpp` hung for over 5 minutes after "taking the census
  with `demo_bringup:linux`", and a re-run hung the same way (killed at 120 s).
  The process tree shows the cause: the configure holds the project lock, its
  census spawns `nros build demo_bringup:linux`, and that child now configures
  implicitly and blocks on the same lock (`locks_lock_inode_wait`), a
  self-deadlock. No stamp was written, so there is no no-op to time. It is a
  W1 defect (the lock and the implicit configure meet in the census child), not
  a slow number. For reference only, the same measurement on
  `examples/workspaces/rust`: first `nros configure` 41.1 s, second
  `nros configure` ("up to date (18 image(s))") **0.064 s** wall, under the 1 s
  target. The C++ number is still to be taken once the census child no longer
  re-enters the lock.

**Not in the W1 change set (still open):** the per-component metadata probe cache,
the per-coordinate stamps for shared host configures, retiring
`NROS_RESOLVED_DIR`, and the parallel-lock measurement. They carry forward
to the later items that consume them.

### W2 — Census by default (RFC-0120 D4)

This flips an existing opt-in to the default. The hook already takes the
census for `entities = "census"` (issue 2086), and that spelling keeps working.

- **C/C++ leaves.** A leaf with no declared `[[component]] entities` gets C4
  automatically. `entities = "defaults"` is the written opt-out.
  `NROS_LEAF_CENSUS_TAKE=OFF` stays, for a driver that already took the census.
- **Rust leaves.** These are counted by the metadata probe in C1 (RFC-0120 D4).
  First measure why phase-481 W3 saw five of six Rust leaves refused, then fix
  what stands between those leaves and a composed inventory.
- **The census's own host build.**
  - It runs on crate defaults, with no seeds and no promise checks, so it takes
    one pass. Today it builds up to three times (`entity_census.rs:376-387`).
  - The workspace census stops spawning `nros build <native>`
    (`entity_census.rs:546-560`) and builds the native image through the host
    tool directly (RFC-0120 D1).
- **Preflight and refusals.** C3 checks the host toolchain: a C/C++ compiler,
  the host Rust target and the store Corrosion. It names the one missing. A
  census that cannot build is refused, naming both fixes.
- **Acceptance:**
  - `examples/zephyr/cpp/talker` is sized from its census on both roads;
  - its xrce image no longer needs the three hand-stated
    `[image.zephyr_xrce.knobs.xrce]` caps: delete them and it still boots.
    `XRCE_MAX_SERVICE_CLIENTS` derives from the sizing descriptor
    (`knobs.toml`), so this also needs the descriptor present on that road.
    The talker comment saying the client cap "has no derivation at all" is
    stale; correct it;
  - a Rust Zephyr leaf is sized from its probe on both roads;
  - a test leaf whose source calls a Zephyr API gets the two-fix refusal;
  - the workspace census reaches no `nros build`.

### W3 — Resolve gaps: the Rust west entry, and the multi-entry union

- **Rust west entry (RFC-0120 D6 case 2).** C5 writes the message-bound seed
  and makes the sizing-descriptor decision for a Rust entry.
  - First MEASURE whether derived message-bound knobs reach the Rust lane's
    cargo environment today. `zephyr/CMakeLists.txt:384-387` already adds every
    derived-rung knob through `nros_derived_knob_env` (issue 1796), so the gap
    may be closed. It remains open only if the bound knobs resolve on another
    rung, or if a Rust entry has no bound producer at all (it calls no
    `nros_find_interfaces`). If a gap is confirmed, file it with
    `just issue-new` and fix it here.
  - Also measure whether a clean generated Rust entry arms the sizing
    descriptor's deferred backstop (`NanoRosSizingDescriptor.cmake:748-765`).
- **Multi-entry (case 3).** C5 resolves once per `cmake_coordinate` as the
  union over its images (`compose_image` already takes several models).
  `nros_resolved_entity_take` accepts N models.
- **Acceptance:** for each of the two, a clean build dir shows 0
  `Re-running CMake`, and the knobs delivered are identical to today's
  converged values.

### W4 — The Zephyr hook calls `nros configure` (cases 1 and 4)

- `nros_image_kconfig_hook` calls `nros configure` for the leaf or for the
  image owning the entry (RFC-0085 D14's walk-up). It returns at once when the
  stamp is current or `NROS_LEAF_CENSUS_HOST` is set. A guest application (no
  `system.toml`) is unchanged.
- Move the issue-1699 Kconfig stamp from `zephyr/CMakeLists.txt` into the
  hook, and add `Kconfig.from-system-toml` to its digest. Add the test it has
  never had: edit `zephyr/Kconfig`, rebuild an existing build dir, and assert
  the new default lands in one pass.
- Replace each early reader's `nros_reconfigure_settle` and each late
  producer's arm with a promise check. The sites are:
  `zephyr/cmake/nros_cargo_build.cmake`, `cmake/NanoRosCodegenCore.cmake`,
  `cmake/NanoRosNodeRegister.cmake`, `cmake/NanoRosEntry.cmake` and
  `cmake/NanoRosSizingDescriptor.cmake`. Retire the pass bookkeeping in
  `cmake/NanoRosResolved.cmake` (`_nros_reconfigure_pass_props`).
- **Acceptance:**
  - a plain `west build` of `examples/zephyr/cpp/talker` and of a Rust leaf,
    and of `examples/workspaces/rust/src/zephyr_entry`, each from a clean build
    dir: 0 `Re-running CMake`, tight knobs;
  - the same three under `nros build`: identical knobs.

### W5 — The CMake and NuttX hooks

- `find_package(nano_ros)` and `add_subdirectory(nano-ros)` call configure
  before any knob reader.
- The NuttX `apps/external` Makefile makes it a prerequisite.
- **Acceptance:** a cmake leaf, a cmake workspace and a NuttX app each build
  from a clean dir in one pass, with knobs identical to `nros build`'s.

### W6 — The census step (or W0's fallback)

- Wire the W0 result into every hooked road: the census step is a build edge
  whose digest is a host-configure dependency.
- **Acceptance:** on the talker,
  - a body-only edit → no re-configure;
  - adding a publisher → exactly one re-configure, and the new count is
    delivered.

### W7 — Delete the fixed point

- Map the fifth producer first: the Cyclone C++ configure-time facts (issue
  1653; `NanoRosSizingDescriptor.cmake`'s `west-cargo.toml` settle and C++
  arm). Give it a pre-configure producer or a promise check.
- Then delete `cmake/NanoRosReconfigure.cmake` and every call site.
- Retire or rewrite `tests/cmake-reconfigure-tests.sh` and
  `scripts/check-knob-fixpoint.py`. The behaviour they pinned no longer
  exists; their replacement asserts 0 re-configures.
- **Acceptance:**
  - `git grep nros_reconfigure_` is empty;
  - `check-knob-delivery` is green;
  - every W3 to W5 measurement is re-run on the final tree;
  - `just ci gate` is green.

### W8 — Vocabulary and docs (RFC-0120 D9)

- Use "configure" for `nros configure` and "host configure" for CMake's,
  west's or make's own step, consistently across RFC-0094, CLAUDE.md, AGENTS.md
  and the book.
- `book/src/reference/cli.md` gains `nros configure`.
- The integration chapters show the main path first and their native tool
  second.
- `how-integration-works.md` states RFC-0072's narrowed scope.
- **Acceptance:** `just book` builds; the probe tracks for the Zephyr and
  first-project chapters pass.

### W9 — Retire `nros sync` (RFC-0120 D2)

`nros sync` is a temporary alias, not a permanent one (maintainer decision,
2026-11-03). It retires in three steps, the way the `ZPICO_*` knob spellings
did (phase-484 W8). Each step is its own change.

1. **Alias, with a warning on every use (LANDED with W1, 2026-11-03).**
   - The warning is printed on stderr whether or not stderr is a terminal, so
     CI logs and scripts see it too. Today `note_old_spelling` prints only on
     a TTY; `sync` gets the always-on form.
   - The warning names `nros configure`.
   - The W1 sweep leaves zero `nros sync` uses in the scanned tree, and
     `check-cli-old-spellings` keeps it at zero.
2. **Refusal (after one tagged release has shipped step 1).**
   - `nros sync` exits non-zero, printing `nros sync was renamed: run
     nros configure` and the same arguments spelled for configure.
   - `tests/`, the `just` recipes, the fixture scripts and the book reach no
     `sync` (the gate already holds this).
   - The release notes name the rename.
3. **Removal (after one more tagged release).**
   - The `Sync` variant, its alias row and its `old_spelling` arm are deleted.
   - `check-cli-old-spellings` keeps refusing the spelling, with a
     `retired` reason, so a reintroduced `nros sync` in a doc or script is
     caught.

- **Acceptance:**
  - step 1: a non-TTY run of `nros sync` prints the warning;
  - step 2: `nros sync` exits non-zero with the message, and every lane is
    green;
  - step 3: `nros sync` is an unknown command, and the gate refuses the
    spelling.
- Steps 2 and 3 wait on releases, so the phase can close with them still
  open, provided W9 names the release each one waits for.

## Order

1. W0 decides W6's shape, so it comes first.
2. W1 and W2 are independent of W0.
3. W3 needs W1.
4. W4 and W5 need W1 and W2.
5. W6 needs W0 and W4.
6. W7 comes last but one, and W8 last.
7. W9 step 1 lands with W1; steps 2 and 3 follow tagged releases.

## What this phase does not do

- Hookless IDE support: the Eclipse pre-build emitter, the rust-analyzer
  include, debug configurations, and the IAR/Keil drop-in (RFC-0120 D8). A
  later phase.
- Moving build hand-off into RFC-0119 runner packages (RFC-0120 open
  question 2).
- Recording the ament install for IDE-launched configures (open question 3).
  It is filed if W4 measures that it is needed.

## Acceptance for the phase

- Every road in W3 to W5 configures in one pass from a clean build dir, and
  after each W6 edit.
- A plain `west build` and `nros build` deliver identical, tight knobs for the
  same image.
- `NanoRosReconfigure.cmake` is gone.
- `nros build` needs no `nros sync` typed by hand.

## Issues homed here (2026-11-04)

Each issue has a `**Home:**` line pointing here.

| issue | work item | why it belongs here |
| --- | --- | --- |
| [#2166](../issues/2166-metadata-probe-recorder-32-entities-panics-on-parameters.md) | W2 | The C/C++ metadata probe's recorder holds 32 entities and every parameter is one, so a bigger component panics and `nros sync` reports `no producer`. W2 makes the census the default entity source. |
