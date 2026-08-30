# The CI ladder — one lane per affordability tier (RFC-0061 / phase-318).
#
# `just ci <lane>`. The tiers are a LADDER: each is a strict superset of the one
# below, and the point of having several is that "run the tier your change
# earns" is an instruction someone can actually follow. A single expensive tier
# is one nobody can afford per task, so it gets followed selectively — which is
# worse than a smaller instruction followed honestly.
#
#   just ci          tier 1 (the default recipe below) — host, builds fixtures
#   just ci l1       compile + unit, NO fixtures                       ~6 min
#   just ci l3       cross build + link + symbols, no QEMU
#   just ci matrix   tier 2 — 1-wise cover over platform/lang/rmw/kind
#   just ci matrix-nightly   pairwise cover (~70%)
#   just ci full     tier 3 — the whole matrix
#   just ci fast     the source-gate tier alone
#
# `mod`, not `import`: these are the one family where a NAMESPACE reads better
# than flat names, because "which tier" is the question a person is actually
# asking. Phase-399 chose `import` for the 200 `check-*` gates on the opposite
# reasoning — nobody types those, and `mod` would have renamed every call site.
# Here `just ci` keeps working because a module's `default` recipe is what runs
# when the module is named with no argument, and 594 references across 262
# files (most of them historical records of what someone ran) stay true.
#
# The flat `ci-l1` / `ci-matrix` / … spellings survive at the repo root as thin
# forwarders, for the same reason.

set working-directory := '..'

# `just ci` with no lane = tier 1, which is what it has always meant.
#
# It must be the FIRST recipe in this file, not merely the one named `default`:
# `just <module>` runs the module's first recipe. Named `default` and placed
# second, `just ci` silently ran `l1` — a WEAKER tier than it has meant for its
# whole life, which is the one way this rename could have done damage quietly.
[group("ci")]
default: tier1


# L1 — compile, lint and unit tests. No fixtures, no platforms, no QEMU.
[group("main")]
l1:
    @just check cli-fresh check::fast check::build check::api-parity
    @just test-unit
    @echo "CI L1 passed (compile + unit; no fixtures were built or needed)."

[group("ci")]
tier1:
    #!/usr/bin/env bash
    set -euo pipefail
    # phase-395 W19 — COORDINATE-scoped, like tier 2, and no longer
    # `NROS_TEST_SCOPE=native`.
    #
    # The name filter excluded `zephyr` and `threadx` outright — the very
    # platforms `board-support.toml` grants tier 1 — so the lane covered one of
    # the three it promised. No token spelling fixes that: `zephyr` also matches
    # `zephyr_cortex_m_qemu`, which is tier 2. Coordinates select exactly the
    # cover and nothing else, which is the same move phase-340 W3 made for tier
    # 2 and the same reason.
    #
    # One computation reaching the build, the staleness gate and the run — the
    # tier-2 recipe below spells it identically on purpose.
    source scripts/build/fixture-lane.sh
    coords="$(nros_lane_coords_file tier1)"
    coords="$(cd "$(dirname "$coords")" && pwd)/$(basename "$coords")"
    NROS_FIXTURE_LANE=tier1 bash scripts/check-tier-preconditions.sh
    NROS_FIXTURE_LANE=tier1 NROS_TEST_COORDS="$coords" \
        just check rust-rtos-link-check test-all
    echo "CI passed (tier 1 — host-executable platforms; cross coverage needs \`just ci-matrix\`)!"

# L3 — cross build + link + SYMBOL checks. No QEMU.
#
# The insight this lane rests on: `rust-rtos-link-check` already BUILDS
# 32-bit/cross ELFs and then throws them away, having only checked that linking
# succeeded. Those artifacts can answer far more, at no extra build cost.
#
# What a cross ELF proves without ever booting (design doc, "cheapest witness
# per defect class"):
#   * 32-bit layout — the sizes-header mirror class, 0088 -> 0114 -> 0122 ->
#     0123 -> 0245 -> 0268. A host build cannot see it; a 64-bit literal in
#     generated code breaks a 32-bit target and nothing on this machine notices.
#   * linker sections and staticlib DCE — 0155, 0163.
#   * static RAM ceilings — the whole of phase 392, which is meaningless where
#     RAM is unbounded.
#   * allocation-freedom — 0816, via the symbol table.
#
# QEMU is needed for scheduling, timing and real transport behaviour. It is not
# needed for any of the above, which is most of what has actually bitten here.
[group("main")]
l3:
    #!/usr/bin/env bash
    set -euo pipefail
    # Was a DEPENDENCY LIST at the root. A module cannot depend on a root
    # recipe, so it becomes an explicit call — and in a shebang recipe it has
    # to come after the shebang, which must be the first body line.
    just rust-rtos-link-check
    source scripts/build/cargo.sh
    echo "== L3 — interrogating the cross ELFs the link check just built =="
    checked=0
    for elf in \
        "examples/qemu-arm-freertos/rust/talker/target-link-check/thumbv7m-none-eabi/$(nros_cargo_platform_profile freertos)/talker" \
        "examples/qemu-arm-nuttx/rust/talker/target-link-check/armv7a-nuttx-eabihf/$(nros_cargo_platform_profile nuttx)/talker" \
        "examples/threadx-linux/rust/talker/target-zenoh/$(nros_cargo_platform_profile threadx-linux)/talker"
    do
        if [ ! -f "$elf" ]; then
            # Not a skip to paper over: the link check reports its own SKIP when
            # a cross toolchain is absent, and this loop must not turn that into
            # a silent pass. Say which artifact is missing and why that is fine.
            echo "  [absent] $elf — its toolchain was skipped upstream"
            continue
        fi
        echo "  mem-report --check: $elf"
        python3 scripts/nros-mem-report.py "$elf" --check
        checked=$((checked + 1))
    done
    if [ "$checked" -eq 0 ]; then
        echo "L3: no cross ELF was available to check — install a cross toolchain" >&2
        echo "    (`just setup freertos` / `just setup nuttx`) or this lane proves nothing." >&2
        exit 1
    fi
    # phase-391 W1/W4 (issues 0816/0843) — the heap-free tier, on a REAL image.
    # `heap-free-poc-mps2` calls `Executor::open_in` + `install_node_typed_in`
    # over per-class static storage and links with NO allocation symbol at all
    # (no `#[global_allocator]`, no `malloc`, no `nros_platform_alloc`). Three
    # earlier probes passed this gate vacuously at `symbols read: 1`; this one
    # reads the full symbol table of a linked image (~460). The build doubles
    # as issue 0843's tripwire: the leaf compiles `nros` WITHOUT the `alloc`
    # feature, so re-coupling the macro-path runtime to `alloc` breaks it.
    echo "== L3 — heap-free tier: link gate on the no-alloc image =="
    (cd packages/testing/nros-tests/bins/heap-free-poc-mps2 \
        && cargo build $(nros_cargo_profile_arg_string))
    python3 scripts/check-no-alloc-image.py --tier heap-free \
        "packages/testing/nros-tests/bins/heap-free-poc-mps2/target/thumbv7m-none-eabi/$(nros_cargo_target_profile_dir)/heap-free-poc-mps2"
    echo "L3 passed — $checked cross ELF(s) checked without booting anything, and the heap-free image links clean."

# Tier 2 — phase-318 W4.d. Gate exactly the fixture COORDINATES the lane selected.
#
# The selection is 1-wise over platform x lang x rmw x kind (`nros_tests::ci_lane`),
# computed from `matrix::CELLS` and emitted by `lane-coords`. 14 of 50 coordinates
# (the count is gated by `ci_lane::tests::documented_lane_table_is_live`; do not
# hand-edit it here — run `lane-coords tier2 | wc -l`).
#
# Why 1-wise and not pairwise, which is what this lane originally specified: cost
# is COORDINATES, not cells, because cells share fixtures and fixtures are what
# take hours. The pairwise cover is 37 of 194 cells (19 %) but 37 of 50
# coordinates (74 %) — a middle tier costing 74 % of the sweep is one nobody runs,
# which is the failure mode RFC-0061 exists to fix. The pairwise coverage moved to
# `ci-matrix-nightly` rather than being dropped: platform x lang is exactly where
# the 0268 / 0245 / 0332 class lives.
#
# Note the gate and the BUILD read the same coordinate file, so they cannot
# disagree about what this lane covers.
#
# Issue 0393 / 0482 / phase-340 W3 — this lane's BUILD is its own lane:
#
#     just build-test-fixtures lane=tier2 && just ci-matrix
#
# That was false until phase-340 W3 and cost ~231 STALE failures when someone
# tried it: 0368 F8 had made `_require-fixtures` accept a `lane=tier2` stamp
# while `ci-matrix` still ran the WHOLE suite, so 34 of 47 coordinates were
# resolved and none of them had been built. 0482 fixed the honesty half by
# demanding an `all` build; W3 fixes the affordability half by narrowing the RUN
# to match the build instead.
#
# The narrowing is NOT name-based — `lane-filter.sh` selects platform families
# and this lane contains every platform (issue 0357 / 0482). It happens in the
# fixture RESOLVER: `NROS_TEST_COORDS` hands it this lane's coordinate file and
# a fixture whose `examples/fixtures.toml` row sits outside it reports SKIPPED
# instead of missing. Build-set and run-set are then one computation — the same
# `row_coord` against the same coordinate file — which is the invariant issue
# 0482 exists to protect. See `nros_tests::fixtures::lane`.
[group("ci")]
matrix:
    #!/usr/bin/env bash
    set -euo pipefail
    just _lane-gate tier2
    # issue 0368 F8 — tell the inner gates that this run is the tier-2 lane, not
    # the default `all`. Without it they DISAGREE: `_lane-gate tier2`
    # (content-based, over the tier-2 coordinates) passes, then the staleness
    # gate inside test-all audits the whole tier-3 fixture set and dies demanding
    # out-of-lane freshness the tier ladder said to skip.
    #
    # NROS_TEST_COORDS is the RUN's half of the same fact, and it is the SAME
    # file `nros_lane_coords_file` gives the build and the staleness gate — one
    # computation reaching all three, not three spellings of a lane.
    source scripts/build/fixture-lane.sh
    coords="$(nros_lane_coords_file tier2)"
    coords="$(cd "$(dirname "$coords")" && pwd)/$(basename "$coords")"
    NROS_FIXTURE_LANE=tier2 NROS_TEST_COORDS="$coords" \
        just check rust-rtos-link-check test-all
    echo "CI passed (tier 2 — 1-wise cover; pairwise interactions need \`just ci-matrix-nightly\`)!"

# Tier 2 nightly — the pairwise cover over platform x lang x rmw x kind (37 of 50
# coordinates). The interaction coverage `ci-matrix` gives up to stay affordable:
# same class of defect, caught a day later instead of pre-merge.
#
# This monolithic form is for a machine that has every SDK — i.e. a developer's.
# In CI the same lane runs DISTRIBUTED across the 07:00 nightly cron, because the
# per-platform toolchains do not coexist on one runner: `.github/workflows/
# nightly.yml` computes the cover with `lane-coords tier2-nightly`, derives the
# platform matrix from it, and its `lane-coverage` job asserts every module in the
# cover actually has a job. Change the lane here and CI follows, with no second
# edit — that is the whole reason the selection is computed.
[group("ci")]
matrix-nightly:
    #!/usr/bin/env bash
    set -euo pipefail
    just _lane-gate tier2-nightly
    # issue 0368 F8 (same class as ci-matrix) — require the tier2-nightly lane so
    # the inner `_require-fixtures` accepts a per-family build instead of
    # demanding the `all` stamp the lane ladder said to avoid. phase-340 W3 —
    # NROS_TEST_COORDS narrows the RUN to the same cover, so that acceptance is
    # earned rather than asserted.
    source scripts/build/fixture-lane.sh
    coords="$(nros_lane_coords_file tier2-nightly)"
    coords="$(cd "$(dirname "$coords")" && pwd)/$(basename "$coords")"
    NROS_FIXTURE_LANE=tier2-nightly NROS_TEST_COORDS="$coords" \
        just check rust-rtos-link-check test-all test-ignored
    echo "CI passed (tier 2 nightly — pairwise cover)!"

# Tier 3 — everything. The former `ci`.
#
# phase-318 W5.c — `test-ignored` (added by #328's fix, e7e5b84a0) joins the lane.
# Those tests had NO lane at all: they rot invisibly and then block the day
# someone enables them, which had already happened — the codegen compile-check
# had been failing since phase-303 W4 added the XCDR2 DHEADER seam, found only
# when 0345 ran it by hand and repaired the stubs. Green now, which is what makes
# laning it worth doing today when it was not last week.
# Issue 0651 — tier 3 now covers the Zephyr 4.4 rolling line, which until here
# existed only in the nightly. Two steps, cheapest first:
#   check::zephyr-kconfig-symbols  source only, ~2 s, needs `just zephyr
#                                 kconfig-trees` (613 MB of shallow clones)
#   zephyr tier3-cell             one real 4.4 build, needs the west workspace
# Both FAIL rather than skip when unprovisioned. That is the point: a lane that
# skips when unprovisioned reports the same colour as one that passed, which is
# how a `select` correct on 3.7 and misspelled on 4.4 reached the nightly a day
# later, attributed to whatever else moved. A host that cannot provision 4.4
# runs tier 2, which never claimed to cover it.
[group("ci")]
full:
    @just check rust-rtos-link-check check::zephyr-kconfig-symbols test-all test-ignored
    @just zephyr tier3-cell
    @echo "CI passed (tier 3 — full matrix, both Zephyr lines)!"

# Fast per-push CI gate: the dependency-free lint/check lane — no heavy builds,
# fixtures, QEMU, network, or ROS install. Runs anywhere. The heavier per-job
# mirrors are separate recipes you invoke when their prereqs are present:
#   just check sdk-index   (network — downloads + sha256-checks SDK release assets)
#   just scaffold-journey  (builds the CLI + a scaffolded project)
#   just colcon-parity     (needs ROS 2 + colcon on the host)
#   just acceptance        (builds the CLI + a scaffolded project)
# The full heavy lane stays `just ci` / `just test-all`.
# Provision the zenoh router ROS itself ships (RFC-0075 / phase-362).
#
# ONE spelling. `host-tests.yml` carried this twice — once as an if/else, once
# as a `||` chain — in two jobs that need the same thing, which is the second
# spelling CLAUDE.md warns about wearing CI clothes. There is no `just zenohd
# setup` any more because the vendored router was deleted; what remains is a
# distro package, and installing it is the only provisioning step.
#
# Absence is NOT a failure: the zenoh lanes SKIP and announce it, and
# `check-zenohd-router-skips` enforces that they can. A job that always fails
# is worse than lanes that honestly report what the host lacks.
#
# Needs root, which CI has and a workstation should not be asked for — so when
# it cannot install, it PRINTS the command instead of running `sudo` (CLAUDE.md:
# never sudo, tell the user).
[group("ci")]
provision-zenohd:
    #!/usr/bin/env bash
    set -uo pipefail
    distro="${ROS_DISTRO:-humble}"
    pkg="ros-${distro}-rmw-zenoh-cpp"
    if [ -n "$(command -v rmw_zenohd 2>/dev/null)" ] || \
       ls /opt/ros/"$distro"/lib/rmw_zenoh_cpp/rmw_zenohd >/dev/null 2>&1; then
        echo "provision-zenohd: $pkg already present"
        exit 0
    fi
    if [ "$(id -u)" != "0" ]; then
        # The remedy is DERIVED, never hand-written: `ros-rmw-zenoh-cpp` is
        # declared in `nros-sdk-index.toml` `[prereq.*]`, and `nros setup
        # --system` composes the native install command for whatever package
        # manager this host has. `check-sysdep-remedies` enforces that, and it
        # rejected a hand-written apt remedy here on the first run — the same
        # ad-hoc provisioning this change exists to remove. (That gate is a text
        # scan, so it also rejects a comment QUOTING the forbidden command,
        # which is why this one describes it instead.)
        # The protocol, not `echo … ; exit 0`. This step genuinely did not do
        # its job, and issue 0650's rule is that such a step must not report
        # success. `check-lane-skip-protocol` rejected the bare form here on
        # the first run — the third of this repo's own gates to catch this one
        # recipe while it was being written to remove ad-hoc provisioning.
        #
        # CI runs as root, so this branch is the workstation case: nothing is
        # installed behind your back, and the command comes from the index.
        source scripts/build/lane-skip.sh
        nros_lane_skip "not root: $pkg not installed. It is a declared prereq, so 'nros setup --system' prints the install command for this host and --sudo runs it. Without it the zenoh lanes report [SKIPPED:capability]."
    fi
    if apt-get install -y --no-install-recommends "$pkg" 2>/dev/null; then
        echo "provision-zenohd: installed $pkg"
        exit 0
    fi
    # One retry after refreshing the index — a container image's apt lists are
    # often older than the package.
    apt-get update -qq || true
    if apt-get install -y --no-install-recommends "$pkg"; then
        echo "provision-zenohd: installed $pkg (after apt-get update)"
        exit 0
    fi
    echo "::warning::no rmw_zenoh_cpp available — zenoh lanes will skip"

[group("ci")]
fast:
    @just check fast check::no-std
    @echo "ci-fast passed!"
