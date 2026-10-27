# nano-ros gates: fixtures
#
# Test fixtures, the lane/coordinate machinery, and the nextest
# filters that decide what runs.
#
# One of the topic files `just/check.just` imports. The names stay FLAT -- a
# gate is still `just check <name>` -- because `import` merges definitions
# rather than namespacing them, and ~350 references across docs, scripts and
# workflows spell them that way.
#
# Adding a gate here does not touch any other file, which is the point: for
# most of this repo's life every new gate appended to ONE file, and
# `just/check.just` became the worst conflict surface in the tree at 48 of
# 300 commits.

# Issue 0900 (option 3) -- does an image's NROS_EXECUTOR_ACTION_CLIENTS agree
# with what it actually LINKS? The knob decides how many executor arena slots
# are budgeted at the worst-case (ActionClient) size, and the arena is inline on
# the TASK STACK, so `just mem-report` cannot see it: the checkable thing is the
# ENTITY that justifies the size, and `nm` reads a cross ELF perfectly well.
# This is the only half of 0900 that reaches embedded images -- the
# sidecar/metadata half cannot, because a cross-compiled component is
# unprobeable by construction.
#
# NOT a fast-lane gate — `.config/gate-lane-exempt.txt` says so, and
# `build-test-fixtures` runs it. Phase-413 W7 / issue 1001: it reads BUILT
# IMAGES, so on the fast line it examined nothing on every pull request (it
# exited 78 and recorded a SKIP) while costing a developer with a populated
# tree a repository walk that is minutes on a cold page cache. It now runs
# where images are a precondition rather than a coincidence, over roots DERIVED
# from the fixture manifest, and examining nothing is a FAILURE.
#
# ~4 s on a fully built 100 GB dev tree: one `nm` per image, no compilation.
action-client-arena-budget:
    python3 scripts/check-action-client-arena-budget.py

# Per-example rustfmt --check (AST-only, no codegen/deps). The `check.yml`
# per-example-fmt step as a recipe (SSoT).
#
# issue 0320 — this invokes `rustfmt` DIRECTLY, not `cargo fmt`. `cargo fmt`
# shells out to `cargo metadata`, which loads the leaf's `.cargo/config.toml`,
# which does `include = ["…/nros-patch.toml"]` — a file `nros sync` GENERATES
# and `.gitignore` excludes. A fresh checkout does not have it, so on CI every
# leaf died with:
#
#   error: could not load Cargo configuration
#     failed to load config include `../../../../../nros-patch.toml`
#
# That held `pr-checks` red on main for 60+ consecutive runs. Since phase-315
# the same call also needs the generated `nros-selection` facade crates,
# because a workspace member's `[dependencies]` path-dep must exist for
# `cargo metadata` to resolve at all.
#
# The deeper problem was placement: this gate lives in `check-fast`, whose
# contract is "BUILDLESS, SOURCE-FREE … no cargo tree/metadata". `cargo fmt`
# violated that. Formatting needs no dependency graph, so calling `rustfmt`
# per file honours the tier and cannot regress this way again.
[private]
example-fmt:
    #!/usr/bin/env bash
    set -e
    # Enumerate via the git index (tracked files only) — no filesystem
    # traversal, so the multi-million-file build/target trees (untracked by
    # definition) can never slow this down or leak in. `NF>=5` = the old
    # `-mindepth 4` (leaf crates, not workspace-root manifests).
    git ls-files 'examples/**/Cargo.toml' | awk -F/ 'NF>=5' \
        | grep -vE '/(target|generated|build|build-[^/]*|install|log)/|examples/zephyr/|/multi-package-workspace/|esp32-c3-baremetal/rust/dds/|examples/mps2-an385-freertos/|examples/qemu-armv7a-nuttx/|examples/threadx-linux/|examples/rv-virt-threadx/|examples/px4/' \
        | sort | while read -r toml; do
        dir="$(dirname "$toml")"
        # Read the edition from the manifest rather than assuming one: rustfmt
        # needs it explicitly (cargo fmt used to supply it), and the tree is
        # not uniform — the zephyr leaves are not 2024.
        edition="$(sed -n 's/^edition[[:space:]]*=[[:space:]]*"\([0-9]*\)".*/\1/p' "$toml" | head -1)"
        edition="${edition:-2024}"
        # Tracked .rs files only — the same index-driven discipline as above,
        # so generated/ and build trees cannot leak in.
        mapfile -t files < <(git ls-files "$dir/*.rs" "$dir/**/*.rs")
        [ "${#files[@]}" -eq 0 ] && continue
        echo "  fmt $dir (edition $edition, ${#files[@]} files)"
        rustfmt "+{{NIGHTLY}}" --check --edition "$edition" "${files[@]}"
    done

# issue 0718 — an example leaf builds in-tree (RFC-0026), so every build
# directory a recipe gives it must be named in that leaf's own .gitignore.
# The six threadx rust leaves ignored `build-cyclonedds` and not `build-zenoh`,
# and their c/cpp siblings ignored both, so no single leaf looked wrong.
[private]
example-leaf-build-dirs:
    @python3 scripts/check-example-leaf-build-dirs.py

# phase-340 P2 — nothing may write an `examples/**/target/` dir. That is the
# invariant item 7 / P4 stands on (`examples/**/target-*/` is globally ignored;
# a plain `target/` survives only through 391 per-leaf .gitignore files), and it
# was being broken by a SECOND build path with no manifest row — hence no
# coordinate, hence no shared cargo group. See the script's header for the
# measurement. Self-tests its own classifier on every run.
[private]
example-leaf-target-dirs:
    @python3 scripts/check-example-leaf-target-dirs.py

# issue 1866 — the cmake side of the same question: every `build-<rmw>/` name
# `Rmw::build_dir()` returns (plus `build/`), under every fixture row's `dir`,
# is ignored — asked of git itself (`check-ignore --no-index`), so a leaf with
# no per-leaf `.gitignore` cannot leave its build tree untracked again.
[private]
fixture-build-dirs-ignored:
    @python3 scripts/check-fixture-build-dirs-ignored.py

# Phase 118.I.5 — keep collapsed examples from regrowing a retired RMW
# directory axis without an explicit documented carve-out.
[private]
example-matrix:
    @bash scripts/check-example-matrix.sh

# issue 1108 / phase-452 W1 — build every copy-out template FROM A COPY.
#
# `examples/templates/` is the one part of the tree a stranger copies, and
# nothing built it: the static gates beside this one ask about the files, and a
# template can satisfy every one of them while `nros build` refuses it. Its
# first run found two real defects that had been green for the template's whole
# life — an unresolvable `<depend>nano-ros</depend>`, and a Rust manifest
# path-depping five levels up into this checkout, so it built in place and
# nowhere else.
#
# On the BUILD lane, not the fast one, because it really does build: the whole
# set measured ~8-9 min locally, one copy at a time, ~4 GB peak in $TMPDIR
# (issue 1453). It reads no fixture stamp and no generated binding; what it
# DOES resolve — the in-tree CLI, `nros-launch-resolve`, the `-sys` sources,
# Corrosion, a ROS `std_msgs`, and for the FreeRTOS sub-projects the pinned
# compiler + kernel + lwIP — is what a job running it must provision, and it
# checks the two tools FRESH before building anything. So it satisfies
# `check-lane-contracts` on a lane that pays for those: since issue 1453 that
# includes the merge queue, via gate.yml's `check-template-copy-out` job
# (`merge_group` + `workflow_dispatch`; `check merge-group-gates` holds the
# placement).
# issue 1058 / 1412 / phase-452 W2 — COMPILE what `nros new` emits.
#
# The scaffold tests verify every variant by substring match (~30 of them) and
# nothing builds the result, so they answer "did we emit the string we meant to
# emit" — a question about the template, not about the scaffold. A user's first
# build is the first compile the emitted code ever gets.
#
# Its first real run found the Rust component template built on the `Component*`
# trait family retired in 212.N.12, broken for three and a half months (#1412).
#
# BUILD lane, not the fast one: ~1 min for six variants warm, and it needs the
# in-tree CLI. It builds everything it resolves, so `check-lane-contracts` holds.
[private]
scaffold-builds:
    @bash scripts/check-scaffold-builds.sh --self-test
    @bash scripts/check-scaffold-builds.sh

[private]
template-copy-out:
    @bash scripts/check-template-copy-out.sh --self-test
    @bash scripts/check-template-copy-out.sh

# Issue 1925 — the two gates above copy a project OUT of the checkout, where
# `find_package(nano_ros)` has no walk-up and a bare `cmake` takes its root from
# the caller's environment: none in a clean shell (configure fails), the PARENT
# checkout in a linked worktree (the gate measures the wrong tree, 1280's
# class). `just` hid it by exporting this checkout's root to every recipe. Every
# gate-script configure goes through `nros_copy_out_cmake_configure`, which
# names THIS checkout; this runs the helper against a stub `cmake` under a
# foreign inherited root, and refuses a bare configure in a gate script.
# FAST lane: no build, no SDK — a stub and a grep.
[private]
copy-out-cmake-root:
    @bash scripts/check-copy-out-cmake-root.sh

# Issue 1926 — a verdict row records a tool's output whatever it says, and a
# stale in-tree `nros` refusing to run was recorded as a configure's verdict,
# then reused after `just setup-cli` (the signature watches the CLI's codegen
# fingerprint, not its source stamp). `_cc_write_verdict_stamp` refuses to
# record a CLI self-refusal and `cc_row_reusable` refuses to reuse one; this
# checks the refusal markers still occur in `stale_guard.rs`, that the stamp
# has one writer, and runs the real bodies against staged dirs.
# FAST lane: no build — a grep and three staged dirs.
[private]
verdict-cli-refusal:
    @bash scripts/check-verdict-cli-refusal.sh

# Issue 1025 -- a packer must not invent the row's cargo args or env. The shared
# cargo group key is a function of (platform, args, env); a consumer supplying
# two of the three gets a directory the build never wrote to, and only once that
# row gains a variant. Silent until then, which is how every ESP32 QEMU flash
# image stopped being packable without a single lane going red.
[group("check")]
fixture-artifact-dir-inputs:
    @python3 scripts/check-fixture-artifact-dir-inputs.py

# Issue 1581 — every embedded cargo IMAGE row bakes a locator its board READS.
# The bringup's `[image.<id>] locator` reaches every board; a row's
# `NROS_LOCATOR` env reaches only boards that `option_env!` it. Four images lost
# their locator when phase-383 deleted the entry packages that carried it, and
# dialled the board default while the test's router listened elsewhere — one of
# them beside a row env that looked like a bake and reached nothing. Buildless:
# the manifest, the bringups, and a grep that VERIFIES the reads-env table.
[private]
image-locator-bake:
    @python3 scripts/check-image-locator-bake.py

# issue 0720 — a test asks a fixture resolver for an artifact BY NAME, and a
# name no CMake target produces resolves to a missing path, which the test
# reports as a `fixture missing` SKIP. So the failure mode is silence, not red.
[private]
fixture-binary-names:
    @python3 scripts/check-fixture-binary-names.py

# Issue 1349 — the SECOND occurrence of one mistake, twenty lines from the first.
# `FixtureVariant::rmw`/`::features` select a row by cargo FEATURES; a crate with
# no `[features]` table can never have such a row, so the selector matches nothing,
# every case resolved `[SKIPPED] fixture not built` (an `unmet!` red since
# issue 1758), and bare nextest rendered that panic FAILED — the cell reads as live failures against a peer it never
# reached. Two interop cells sat unverified that way. Buildless: two Rust call
# sites and a TOML table.
fixture-variant-features:
    @python3 scripts/check-fixture-variant-features.py

[private]
fixture-groups:
    @python3 scripts/check-fixture-groups.py
    @bash packages/testing/nros-tests/tests/fixture_group_collision_gate.sh

# Issue 1707 — the Zephyr fixture lane's ninja path regenerates a retargeted
# row's west application BEFORE ninja, and a configure that read an application
# rewritten under it is refused. Buildless: stub `nros`/`ninja` plus `cmake -P`.
zephyr-app-regen:
    @bash packages/testing/nros-tests/tests/zephyr_fixture_app_regen.sh

# Issue 0406 — a fixture builder narrowed to an id it cannot match must FAIL,
# not exit 0 having built nothing. Buildless: exercises the shared guard and
# one real builder invocation that stops before any compilation.
fixture-id-guard:
    @bash scripts/check-fixture-id-guard.sh

# Issue 1403 — every fixture/jobserver `make` runs under
# `scripts/build/make-stall-watchdog.py`, which turns "make idle with no child"
# into a diagnostic file plus a `NO VERDICT: jobserver stall` exit (rc 75)
# instead of a hang. Runs the REAL watchdog against fake makes: a stalled one
# must be caught (the negative control, every run) and a busy, failing, quick
# or disabled one must pass through untouched. Buildless, ~10 s.
make-stall-watchdog:
    @python3 scripts/check-make-stall-watchdog.py

# Issue 1403 — the watchdog only answers for the makes it WRAPS. Every `make -j`
# / `--jobserver-style` / `make_args` invocation in justfile, just/ and
# scripts/ must carry `make-stall-watchdog.py` in the same command; the sweep
# that resolved 1403 found four that did not. Static, with a self-test.
jobserver-makes-watched:
    @python3 scripts/check-jobserver-makes-watched.py

# Issue 1264 — `nros_platform_rmws` (and the fixture-id-guard functions above,
# which share its `nros_manifest_query` helper) must report a manifest parser
# that failed to even START as a PARSER failure, never as "this platform/id
# does not exist". Buildless: sources the library and calls its functions
# directly, once against a fake `python3` that dies the way a python3.10 host
# with no tomllib/tomli does.
platform-rmws:
    @bash scripts/check-platform-rmws.sh

# Issue 0835 — a fixture staleness probe decides from the ARTIFACT's BYTES, not
# from whether the build did work. Corrosion's cargo step is an always-dirty
# edge and the phase-340 cargo groups evict rows nobody touched, so an
# activity-keyed probe reports work on EVERY run forever: it reported the same
# 17 cmake cells + 23 rust rows stale on a tree whose `lane=all` build had just
# gone green, which is what failed ~190 tests on every `just ci matrix`.
#
# Fixed in 2fa1ed09f with nothing guarding it. This runs the REAL probe scripts
# against a two-file cmake cell and a one-binary cargo leaf, and carries its own
# negative control: the pre-fix decision rules are re-applied to the same trees
# and must FIRE.
fixture-staleness-probes:
    #!/usr/bin/env bash
    set -euo pipefail
    _missing=""
    for _t in cmake ninja cargo cc; do
        command -v "$_t" >/dev/null 2>&1 || _missing="$_missing $_t"
    done
    # The probe asks the CLI for the platform's cargo profile, so the leaf
    # cannot be built without it — and a missing CLI would surface as every case
    # failing on `profile ... is not defined`, which reads as a probe defect.
    # shellcheck source=scripts/build/cargo.sh
    source scripts/build/cargo.sh
    # issue 1850 — $NROS_CLI, else this checkout's CLI; never PATH.
    _cli="$(nros_cli_bin --checkout-only)" || _missing="$_missing nros(just setup-cli)"
    if [ -n "$_missing" ]; then
        # shellcheck source=scripts/build/check-skip.sh
        source scripts/build/check-skip.sh
        nros_check_skip fixture-staleness-probes "missing:$_missing"
        exit 0
    fi
    export NROS_CLI="$_cli"
    ./tests/fixture-staleness-probe-tests.sh

# phase-489 W6 — a fixture group's single-package leaves each get their sizing
# class's own cargo build dir (`nros ws leaf-system --write-settings
# --target-dir`), in BOTH the build and the staleness probe. Without it every
# row after a different leaf recompiled the core crates (~12.7 s a row, ~50
# rows, on a re-run with no change). Stub CLI; no build needed.
[private]
leaf-row-build-class:
    @./tests/leaf-row-build-class-tests.sh

# A fixture stamp may not claim what the build did not achieve. The stamp used
# to record the lane's NOMINAL coordinates, so a module that skipped a missing
# prerequisite still read as covered and the run skipped its tests to a green
# sweep (phase-411 W1).
# Issue 1582 — two images, or two west leaves, that BUILD INTO ONE OUTPUT.
# `realtime-rust` declared `[image.zephyr]` in two bringups; a generated entry
# is keyed on the image id, so both west leaves were configured on one
# `build/zephyr-zenoh/zephyr_entry/` application and the authored-tier image
# booted the derived-tier content. `check-west-leaf-vocabulary` asks whether a
# NAME is modelled, not whether two names share an input. Static: tracked
# `system.toml`s + `fixtures-manifest.py west-leaves`, ~0.2 s, self-testing.
[private]
generated-output-collisions:
    @python3 scripts/check-generated-output-collisions.py

[private]
fixture-stamp-honesty:
    @python3 scripts/check-fixture-stamp-honesty.py

# The fixture manifest's own validators — every `[[workspace_fixture]]` and
# `[[compile_check_fixture]]` row must name files that EXIST and a target the
# CMakeLists/Cargo.toml actually defines.
#
# These validators shipped with no caller: `git grep validate-workspaces` found
# only the script's own usage text and dispatch. Unrun, they drifted until 74 of
# 86 workspace rows failed — the detector still looked for pre-RFC-0048 verbs and
# demanded a `[system].default_launch` that phase-296 retired. Both were checker
# staleness, not fixture breakage, and nothing could tell, because nothing ran.
# That is the issue-0309 silent-lane class: a gate nobody wires in decays into
# noise, then into a gate you cannot afford to turn on.
#
# Buildless and source-free (path existence + regex over tracked files), ~0.1s
# for all 112 rows, so it belongs in the per-push fast tier.
[private]
fixtures-manifest:
    @python3 scripts/build/fixtures-manifest.py validate-workspaces
    @python3 scripts/build/fixtures-manifest.py validate-compile-checks
    # Issue 0482 — plain `[[fixture]]` rows had NO validator at all, while the
    # other two kinds had one. A row missing `platform` or `lang` has no
    # coordinate, so it keeps building under `lane=all` and silently leaves
    # every coordinate-scoped lane; a row naming a typo'd `rmw` lands on a
    # coordinate no lane and no matrix cell can hold.
    @python3 scripts/build/fixtures-manifest.py validate-fixtures
    # phase-350 W0 (issue 0538) — `fixture-inventory.py`'s hand-authored list
    # asserts "this build is NOT in the manifest" for each of its rows, and had
    # NO consumer, so four of them went on asserting it after the build migrated
    # in — one of them a row phase-344 W2 added for exactly that reason. Same
    # shape as `examples_fixture_coverage.rs`'s stale-exception arm: an
    # exception that came true must fail, or the list rots into a false
    # negative read as authoritative precisely when someone audits coverage.
    @python3 scripts/build/fixture-inventory.py --check
    # phase-350 W1 (issue 0535) — the zephyr west leaves now have manifest rows,
    # but `zephyr-fixture-leaves.sh` still derives its own matrix, so the two are
    # briefly two spellings of one thing. This makes them unable to drift until
    # the emitter reads the rows. Read-only: the emitter runs no build tool.
    @python3 scripts/check-zephyr-fixture-rows.py

[private]
lane-scope-consumers:
    @python3 scripts/check-lane-scope-consumers.py

# Issue 1900 — a test of a platform the lane did not select DESELECTS
# (`[SKIPPED:lane]`) whatever the host has: the scope check comes before any
# probe (issue 1758). Runs every platform-token `nros-tests` test under the
# `linux` scope with the emulators and RTOS trees hidden, and fails any that
# read the host first (`[UNMET PRECONDITION]`, a hand-written `[SKIPPED]`).
# A test that RAN and failed with no marker — an assertion, or a nextest
# TIMEOUT — is its own class with its own remedy (issue 2069): a failure
# fails; a timeout fails only if `test-lane-contracts` does not also run that
# test (`.config/lane-admission/gate.txt`), otherwise it is printed as a
# warning. Needs no fixture: an out-of-lane test stops at its first statement, and the
# two the scope admits that need a fixture are listed, not failed. ~30 s of
# run on top of the `nros-tests` test-binary build. Step of `just ci gate`.
[group("ci")]
out-of-lane-deselected:
    @python3 scripts/test/check-out-of-lane-deselected.py

# phase-433 W4 / issue 1127 — `matrix_fixture_coverage.rs` G1 promises "a Runtime
# cell nothing runs ... fails here" and asserts that `tests/<test>.rs` EXISTS.
# This is the other half: every `Tier::Runtime` cell in `interop::CELLS` names a
# test binary some `just` recipe or workflow actually INVOKES. 8 of 17 did when
# it was written; the other 9 are `.config/interop-cells-without-runner.txt`,
# which only shrinks. Buildless — parses `interop.rs`, reads the justfiles and
# the workflows.
[private]
interop-cell-runners:
    @python3 scripts/check-interop-cell-runners.py

# phase-433 W5 / issue 1127 — the half that outlives W4's runner gate. A cell can
# be aimed at and still never have met a peer: a skip becomes `<skipped>`, every
# consumer counts that as not-a-failure, and a cell that has skipped on every host
# since the day it was written is indistinguishable from one that passes. So what
# a live peer has ANSWERED is written down (`.config/interop-verdicts.toml`) and
# an absent entry means NEVER RUN — the default, so nothing here is satisfiable by
# a cell that never runs. This gate checks the ledger's integrity only, which is
# why it is green on a host with no ROS: real cell, required fields, cases that
# still exist, an OPEN issue behind every `fail`, and no BINDING test cited as
# evidence (`cases_bound_to_interop_cells` passes with no peer, forever — citing
# one would put the defect inside the fix). Issue 1191 added the half that says
# WHICH cell a case belongs to: four binaries host more than one Runtime cell
# (interop_e2e hosts five), so `interop::CASE_CELLS` maps case → cell and this
# gate checks that map against the cases each binary actually runs, both ways.
# Report: `just interop-verdicts`.
[private]
interop-verdict-ledger:
    @python3 scripts/check-interop-verdicts.py

# Issue 0743 fallout — a `binary()` in .config/nextest.toml naming a deleted test
# target makes nextest refuse to PARSE the config, which kills every nextest run
# in the repo, not just that lane. It went unnoticed behind a green `just check`
# because `just check` does not run nextest; this gate is that missing coverage.
# `test()` names are deliberately NOT checked — they are rstest-generated case
# names that appear nowhere in the sources (see the script's header).
[group("ci")]
nextest-binary-filters:
    @python3 scripts/check-nextest-binary-filters.py

# Issue 2004 — a test that runs a matrix of cells in one `#[test]` PASSES over
# the cells outside the run's coordinates, and its stdout summary is the only
# record of which cells ran. nextest discards a passing test's output, so every
# such test must sit in the `success-output = "final"` override. The subject is
# derived: every test file calling `lane_scope::skip_note(` for a summary.
[group("ci")]
matrix-summary-visible:
    @python3 scripts/check-matrix-summary-visible.py

# Issue 1899 — a compile the kernel killed prints no diagnostic, so the first
# error line a failing fixture family shows is a consequence. The excerpt
# printer counts kills and names them first; this runs its self-test.
[group("ci")]
log-first-errors:
    @bash scripts/build/log-first-errors.sh --selftest

# phase-373 W1 — every `test()`/`binary()` predicate in .config/nextest.toml must
# match at least one real test. The `binary()` half is already covered statically
# by `check-nextest-binary-filters` on the fast line; this is the `test()` half,
# which that gate cannot do because rstest case names appear nowhere in the
# sources. It needs the test binaries, so it runs from `test-all` rather than
# `just check`, which builds none.
[private]
nextest-test-filters:
    @python3 scripts/check-nextest-test-filters.py

[private]
no-tracked-models:
    @bash scripts/check-no-tracked-models.sh

# issue 1501 — the other half of the model rule. `no-tracked-models` bans
# committing the artifact; this one asks whether the bringup that needs it can
# be REACHED by the only thing that produces it. `nros sync` scans
# `src/<pkg>/package.xml` or a root `package.xml` and rejects a dir with
# neither, so a self-pkg bringup declaring `[[component]]` without a
# `package.xml` has no producer for its model and refuses at every bake.
[private]
self-pkg-package-xml:
    @python3 scripts/check-self-pkg-package-xml.py

# phase-383 W10.c — an example workspace ROOT is generated, never tracked.
#
# `nros build` emits it from the discovered packages plus `[image.*]` (RFC-0065
# D3); a tracked one is used AS-IS instead, so it silently overrides the
# declarations it came from. The class is "someone re-adds a hand-written root",
# and it already happened once during the migration that removed them — a
# `git add -A` re-added three, which `.gitignore` cannot prevent because it does
# not apply to files git already tracks.
[private]
no-tracked-workspace-roots:
    @python3 scripts/check-no-tracked-workspace-roots.py

# Issue 0652 — a `required-features` target no recipe enables is invisible:
# cargo does not report it as filtered, it simply never builds it, so it looks
# like coverage while running nothing. phase-510 W1 (RFC-0051 §7b) made the
# unit the TARGET, not the feature: every feature-gated test target, in every
# crate, must sit in a `.config/feature-lanes.toml` lane whose features satisfy
# its gate. The feature-level check this replaced passed while four targets
# behind reachable features ran nowhere.
[group("main")]
required-features-reachable:
    @python3 scripts/feature-lanes.py check

# A test that can SKIP must be run by a runner that understands skips.
# `nros_tests::lane_skip!` PANICS with a `[SKIPPED:lane]` marker (Rust's harness
# has no native skip), and only `_nextest-tolerant` rewrites it before tallying —
# so a bare runner turns an out-of-scope test into a red naming a test the
# change never touched.
[private]
skippable-tests-tolerant:
    @python3 scripts/check-skippable-tests-tolerant.py

# issue 0445 / 0442 — every freshness probe shares ONE exemption rule and ONE
# verdict. A staleness verdict is absorbing: the fixture never launches, so the
# runtime result it would have produced is replaced by a self-explaining
# message. Issue 0444 hid behind 0442 (an exemption applied on one probe arm
# and not its sibling) for exactly as long as those cells read STALE.
[private]
staleness-probe-exemptions:
    @bash scripts/check-staleness-probe-exemptions.sh

# issue 1129 / phase-450 W1 — converting a fixture-resolver `Err` is ONE
# decision and belongs in one place. This keys on the CALL (`require`), not on
# the wording a site chose, because keying on wording is exactly what it
# replaces: `check-skip-budget` grepped `not prebuilt` and saw 4 of 61 sites,
# because 57 said `not built`. A 62nd wording cannot reopen a rule that never
# read the words.
fixture-require:
    @python3 scripts/check-fixture-require.py

# issue 1903 — a `require_prebuilt_artifact` path is in no manifest row, so the
# lane resolver can never call it out of lane: every gated sweep that reaches the
# call claims the artifact. Its `remedy` must therefore be a recipe the lane
# build (`build-test-fixtures`) runs, unless every binary reaching the site is
# one `test-all` excludes (an opt-in lane that builds before it runs).
# `cyclone_slirp_pair` named `just cyclonedds build-rmw`, which only
# `check rmw-cyclonedds` ran, so `just test linux` failed it as a broken promise.
# Sites, forwarding helpers, exclusions and the closure are all DERIVED.
prebuilt-artifact-producers:
    @python3 scripts/check-prebuilt-artifact-producers.py

# Issue 0580 — a test must ASSIGN its ROS domain, never name one: a literal is a
# shared bus, and two concurrent runs colliding on it presents as WRONG DATA
# rather than as a collision.
test-domain-assignment:
    @bash scripts/check-test-domain-assignment.sh

# Issue 0991 -- the two inventories above are written LATER in a configure than
# they are READ, and the recovery three call sites described in comments
# ("CMAKE_CONFIGURE_DEPENDS, so ninja re-runs cmake by itself") was measured to
# never fire: `build.ninja` is written after the fragment, so the dependency is
# never stale. A memory-tight image therefore sized itself from the "nothing
# composed yet" placeholder on every clean build dir and failed at LINK naming
# 103160 bytes and no knob.
#
# This gates the mechanism that replaces it. It is a REAL configure plus a real
# `ninja`, not `cmake -P`, because the claim is about what ninja does with an
# mtime -- and the thing being fixed is a mechanism that read as working in
# every review while firing never. The control case reproduces that bug, so the
# gate would notice a fix that quietly stopped working.
#
# cmake + ninja only: `project(... NONE)`, no compiler, no nano-ros build, no
# codegen, ~2 s. Fast line, beside the two readers it protects.
# Issue 0870's neighbour, found while restoring `declared-qos-header` above: a
# test script with NO CALLER is a test that runs nowhere, and nothing detected
# it. `e569d9a55` -- a commit about which Zephyr SDK the CI image bakes --
# deleted that recipe and its lane entry, its message mentioning neither, and
# `tests/cmake-declared-qos-header-tests.sh` sat tracked and unreachable for a
# day while guarding phase-403 step 2's delivery half.
#
# The repo's recurring class one level up: a mechanism that is correct and
# unreachable. Every previous instance (0896, 0963, the flag-only selftest, the
# NuttX printk arm, `just xrce setup`'s short-circuit) was found by a human
# noticing, which is not a control.
#
# Pure `git ls-files` + line scan, no build, ~0.2 s -- fast line.
test-scripts-have-callers:
    python3 scripts/check-test-scripts-have-callers.py

# Issue 1137 — if the PEER's bus is pinned, OUR side's must be too.
#
# Issue 1009 confined every DDS interop peer to loopback, and measured that
# pinning ONE side is worse than pinning neither (0 of 15, empty output). It was
# then half-applied: `ros2_env_setup_rmw_with_domain` builds a shell string, so
# the pin reached the `ros2` peer and never the bare `std::process::Command` our
# own binaries are spawned from. Two days later
# `cyclone_enumerates_a_stock_ros2_node` — which had PASSED live on 2026-08-30
# (issue 0927) — enumerated exactly one node, itself, against a stock talker
# that was up, because the talker sat on `127.0.0.1` with multicast off while
# our probe kept Cyclone's default interface. That symptom is
# indistinguishable from a broken `ros_discovery_info` reader, and was filed as
# one twice.
#
# Keys on the HOST peer helpers, never on the `Middleware` enum: `DockerRosEnv`
# names the same variants and its pairs are symmetric-UNPINNED (a container
# cannot read a host profile path), so pinning our half of one would create the
# bug rather than fix it.
#
# Pure `git ls-files` + line scan, no build, no ROS — fast line.
dds-isolation-symmetry:
    python3 scripts/check-dds-isolation-symmetry.py

# Issue 1671 -- a lane that sets NROS_FIXTURE_FAILED_ROWS builds every fixture
# row past a failed one and records it; `set -e` still holds inside each row; the
# owner (`scripts/ci/fixture-rows-keep-going.sh`) fails on any recorded row; and
# both shared builders route every row through `nros_fixture_row`. Negative
# controls run on the normal path.
[private]
fixture-row-keep-going:
    @bash scripts/check-fixture-row-keep-going.sh

# issue 1650 (phase-477 W2) — every example root is BUILT by some fixture row,
# or records why not. Keyed on the census's SHAPE (examples/<plat>/<lang>/<name>,
# examples/workspaces/<ws>, examples/templates/<t>), never on a path list; the
# manifest is read through fixtures-manifest.py's own loaders. ~0.1 s, tracked
# files only; ratcheted reasons in `.config/example-build-coverage-baseline.txt`.
example-build-coverage:
    @python3 scripts/check-example-build-coverage.py

# Issue 1845 -- every `nros-sdk.lock` a build can write in this checkout is
# gitignored. An unignored one is a `nros_source_manifest` input, so each entry
# build re-staled its sibling workspace fixtures. Asks git per workspace and
# project directory, and self-tests the old root-only rule on a throwaway repo.
sdk-lock-ignored:
    @bash scripts/check-sdk-lock-ignored.sh

# Issue 1862 -- the workspace fixture builder refuses a row whose build left a
# file in the workspace that git neither tracks nor ignores, because the
# signature hashes it and it re-stales every row built there before. The ignore
# rules (`sdk-lock-ignored`, `fixture-build-dirs-ignored`) cover the writers
# known so far. This one is about what a build actually writes. The gate runs
# the helper's selftest: a new write is refused, while an ignored write and a
# file present before the build are not.
unignored-build-writes:
    @bash scripts/build/unignored-writes.sh selftest

# Issue 1325 -- `NROS_ZEPHYR_FIXTURE_FILTER` is matched per FIELD and
# repo-relative, in both Zephyr builders, through one helper
# (`scripts/lib/fixture-filter.sh`). A joined haystack let the checkout's own
# path and cross-field `.*` select every leaf, silently and at build cost. The
# gate runs the real leaf emitter from a checkout path named `448-xrce`.
fixture-filter-fields:
    @bash scripts/check-fixture-filter-fields.sh
