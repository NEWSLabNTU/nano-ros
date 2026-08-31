#!/usr/bin/env bash
# The SCOPE vocabulary — phase-411 W3.
#
# Sourced, not executed. One namespace for the thing every verb takes in its
# first argument position:
#
#   just setup  <scope…>      provision it
#   just doctor <scope…>      is it ready?
#   just build  <scope…>      build its fixtures
#   just test   <scope…>      run its tests
#
# `just setup <platform>` was ALREADY verb-first; this is the rest of the
# surface following the half that was right. The platform MODULES do not go
# away — `just zephyr build-fixtures` remains the implementation, and the verbs
# below dispatch to it. What changes is that the documented surface is
# verb-first, so there is one shape to learn instead of two.
#
# # One namespace
#
# A scope token is EITHER a platform or a preset, and the two share one
# position, so a name may not mean two things:
#
#   platform   a `mod` in the justfile that owns build/test artifacts
#   preset     a name for a SET of platforms — today exactly the fixture LANES
#              (`_NROS_LANES` in fixture-lane.sh), because a lane already is a
#              named platform set and inventing a second vocabulary beside it
#              is how two spellings of one fact start.
#
# `native` is deliberately in BOTH sets: it is a platform module and a lane
# name. That is legal precisely because they denote the SAME scope — the
# `native` lane is every row of the `native` module, which `nros_lane_modules`
# spells out. `check-scope-namespace` asserts it, so a future preset that
# collides with a platform meaning something else fails on the fast line
# instead of silently re-scoping somebody's run.
#
# # The verb reaches the scope through its own machinery
#
# A token names a SCOPE, not an implementation. Each verb then uses whatever
# already exists for that scope:
#
#   build  a lane token goes to `build-test-fixtures lane=…` (which writes the
#          coverage stamp `_require-fixtures` reads); a platform token goes to
#          `just <plat> build-fixtures`, the same call that recipe's own
#          fan-out makes.
#   test   a coordinate-scoped lane (tier1/tier2/tier2-nightly) and `all` go to
#          the lane run (`test-all` narrowed by `NROS_TEST_COORDS`); a platform
#          token — `native` included — goes to `just <plat> test`, because the
#          module IS that platform's run.
#
# So `native` resolves through the lane for `build` and through the module for
# `test`. Same scope, different machinery, which is the point of naming the
# scope rather than the recipe.
#
# # The default scope is DERIVED, never recorded
#
# "What do I have provisioned?" is a PROBE (`nros_scope_provisioned`), run
# against each module's own `doctor` — the authority that already knows the
# prerequisite and already prints the remedy. It is never read from a file: a
# recorded set is a second source of one fact, and it goes stale in the
# direction that reports coverage nobody has.

# shellcheck shell=bash

_nros_scope_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# The lanes are the presets, so the lane list is sourced rather than copied.
if ! command -v nros_lane_validate >/dev/null 2>&1; then
    # shellcheck source=scripts/build/fixture-lane.sh
    . "${_nros_scope_dir}/fixture-lane.sh"
fi

# ---------------------------------------------------------------------------
# The namespace
# ---------------------------------------------------------------------------

# Platform scope tokens: the justfile modules that own build/test artifacts for
# a target platform or an RMW backend. Ordered host-first, then by how often a
# person names them.
_NROS_SCOPE_PLATFORMS="native zephyr freertos nuttx threadx_linux threadx_riscv64 esp32 esp_idf qemu px4 xrce cyclonedds"

# Modules that are deliberately NOT scope tokens, with the reason — because
# "why is `docker` not a scope?" is the question a reader will have, and an
# unexplained subtraction reads as an oversight. `check-scope-namespace` asserts
# this list plus the one above PARTITIONS the justfile's modules, so a new
# module has to be classified rather than silently landing in neither.
#
#   check ci        verb modules, not scopes (`just check fast`, `just ci l1`)
#   workspace       the ROS/colcon workspace tooling — an environment, not a
#                   target. Also the one module whose `doctor` costs ~75 s, so
#                   probing it per default-scope report would make every bare
#                   `just test` pay for a ROS install probe.
#   verification    Kani/Verus, a proof lane over the host build
#   docker          a RUNNER (`just docker test …` runs a recipe in a container)
#   probe           the book-bootstrap prober
#   ros_editions    the ROS distro axis, not a platform (issue 0327)
_NROS_SCOPE_NON_PLATFORM_MODULES="check ci workspace verification docker probe ros_editions"

nros_scope_platforms() {
    printf '%s\n' $_NROS_SCOPE_PLATFORMS
}

# The presets. Exactly the fixture lanes — see the header for why there is no
# second vocabulary.
nros_scope_presets() {
    printf '%s\n' $_NROS_LANES
}

# Normalize a token as it arrives from `just`.
#
# `just <recipe> lane=tier2` / `tier=all` pass `lane=tier2` / `tier=all`
# VERBATIM as the positional value (just's `name=value` CLI form sets
# variables, not recipe parameters), and both spellings are in the docs, in CI
# and in muscle memory. `nros_lane_arg` already strips `lane=`; this strips the
# scope-position prefixes and maps the setup-tier aliases onto the scope that
# means the same thing.
nros_scope_normalize() {
    local tok="${1:-}"
    tok="${tok#lane=}"
    tok="${tok#tier=}"
    tok="${tok#scope=}"
    case "$tok" in
        # `just setup all` and lane `all` both mean "everything"; the aliases
        # come from `_orchestrate`'s tier table.
        everything | contributor | extended) tok="all" ;;
        # The base setup tier is the host quick start.
        base | quickstart | minimal | default) tok="base" ;;
    esac
    printf '%s' "$tok"
}

nros_scope_is_platform() {
    local tok
    tok="$(nros_scope_normalize "${1:-}")"
    local p
    for p in $_NROS_SCOPE_PLATFORMS; do
        [ "$tok" = "$p" ] && return 0
    done
    return 1
}

nros_scope_is_preset() {
    local tok
    tok="$(nros_scope_normalize "${1:-}")"
    local p
    for p in $_NROS_LANES; do
        [ "$tok" = "$p" ] && return 0
    done
    return 1
}

# Is this token a fixture LANE — i.e. can it be handed to
# `build-test-fixtures lane=…` / `NROS_FIXTURE_LANE`? Every preset is, today;
# the question is asked separately because the verbs ask it, and a future
# preset that is not a lane must not silently become one.
nros_scope_is_lane() {
    nros_scope_is_preset "$1"
}

# platform | preset | unknown.
#
# PLATFORM-first, and only for the human-readable label: `native` is both, and
# "platform" is the more informative of the two true answers. It cannot change
# any resolution, because the two denote the same scope — which is exactly what
# `check-scope-namespace` asserts, and the reason a collision is allowed at all.
nros_scope_kind() {
    if nros_scope_is_platform "$1"; then
        echo platform
    elif nros_scope_is_preset "$1"; then
        echo preset
    else
        echo unknown
    fi
}

# The platform set a preset denotes, one per line.
#
# `all` and `native` answer statically. The tier lanes answer through
# `nros_lane_modules`, which is `lane-coords --modules` — the SAME selection
# the fixture build fans out over, so a tier's scope cannot differ between what
# `doctor` reports and what `build` builds. That costs a `cargo run` when no
# prebuilt selector is current, so `NROS_SCOPE_NO_BUILD=1` makes it refuse
# instead: the gate runs on the fast line, which is buildless by contract.
nros_scope_preset_expand() {
    local tok
    tok="$(nros_scope_normalize "${1:-}")"
    case "$tok" in
        all)
            printf '%s\n' $_NROS_SCOPE_PLATFORMS
            return 0
            ;;
        native)
            echo native
            return 0
            ;;
    esac
    if ! nros_scope_is_preset "$tok"; then
        echo "scope: '$tok' is not a preset" >&2
        return 2
    fi
    if [ "${NROS_SCOPE_NO_BUILD:-0}" != "0" ]; then
        echo "scope: expanding preset '$tok' needs lane-coords, and NROS_SCOPE_NO_BUILD is set" >&2
        return 3
    fi
    nros_lane_modules "$tok"
}

# Every platform a scope token covers, one per line (a platform token covers
# itself). This is the ONE expansion; `doctor` and the reporting both use it so
# a preset cannot mean one set to one verb and another to the next.
nros_scope_expand() {
    local tok
    tok="$(nros_scope_normalize "${1:-}")"
    if nros_scope_is_platform "$tok"; then
        echo "$tok"
    elif nros_scope_is_preset "$tok"; then
        nros_scope_preset_expand "$tok"
    else
        nros_scope_reject "$tok"
        return 2
    fi
}

# Reject the whole argument list before ANY of it runs. A typo in the third
# token must not be discovered after the first two have built for ten minutes,
# which is the same "fail at the point of decision" phase-411 asks of an
# unprovisioned platform.
nros_scope_validate_all() {
    local tok rc=0
    for tok in "$@"; do
        if [ "$(nros_scope_kind "$tok")" = "unknown" ]; then
            nros_scope_reject "$tok" || rc=2
        fi
    done
    return "$rc"
}

# The failure a mistyped scope gets. It prints the whole namespace, because the
# one thing a person needs at that moment is the list of legal words.
nros_scope_reject() {
    local tok="${1:-}"
    echo "unknown scope '$tok'." >&2
    echo "" >&2
    echo "  platforms: $_NROS_SCOPE_PLATFORMS" >&2
    echo "  presets  : $_NROS_LANES" >&2
    echo "" >&2
    echo "  A scope is one word in one position:  just <verb> <scope…>" >&2
    echo "  e.g.  just setup zephyr    just build tier2    just test zephyr" >&2
    return 2
}

# Does `just <module>` define `<verb>`? ASKED, not tabulated — `just --list
# <module>` is the authority, so a module that gains or loses a verb needs no
# edit here. ~50 ms.
nros_scope_module_has_verb() {
    local mod="${1:?nros_scope_module_has_verb: module}"
    local verb="${2:?nros_scope_module_has_verb: verb}"
    just --list "$mod" 2>/dev/null | awk '{print $1}' | grep -qx -- "$verb"
}

# The refusal a scope gets when the verb has no implementation for it. A named
# scope must WORK or say why — reaching `just esp_idf test` and letting `just`
# answer "justfile does not contain recipe" names neither the scope nor the
# verb the person actually typed.
nros_scope_require_module_verb() {
    local mod="${1:?}" verb="${2:?}"
    nros_scope_module_has_verb "$mod" "$verb" && return 0
    echo "scope '$mod' has no '$verb' lane — \`just $mod $verb\` does not exist." >&2
    echo "  What it does have:" >&2
    just --list "$mod" 2>/dev/null | awk '{print $1}' \
        | grep -E '^(setup|doctor|build|build-fixtures|build-examples|test)' \
        | sed 's/^/    just '"$mod"' /' >&2
    return 2
}

# Run the command a scope resolved to — or, under NROS_SCOPE_EXPLAIN=1, print
# it and stop.
#
# The dispatch layer is the one part of this surface whose whole job is to
# choose a command, so "which command does `just build tier2` become?" must be
# answerable without paying for the command. It is also how a CI author reads
# the mapping (phase-411 W4) and how this layer gets verified at all: the
# alternative is starting a multi-hour fixture build to watch the first line.
nros_scope_exec() {
    if [ "${NROS_SCOPE_EXPLAIN:-0}" != "0" ]; then
        echo "would run: $*"
        return 0
    fi
    exec "$@"
}

# ---------------------------------------------------------------------------
# The default scope: what this host has provisioned, PROBED
# ---------------------------------------------------------------------------

# Does `<platform>` have a module `doctor` to probe? `native` is the host — the
# machine you are typing on — so it has no separate provisioning question and
# the root `doctor`'s host block is its readiness report.
nros_scope_has_module_doctor() {
    local p
    p="$(nros_scope_normalize "${1:-}")"
    [ "$p" != "native" ]
}

# rc 0 when `<platform>` is provisioned. The probe IS the module's own
# `doctor`, which already knows the prerequisite and already prints the remedy;
# a second definition of "provisioned" here would be the drift this file exists
# to avoid. Output is captured — callers that want the detail re-run the doctor.
nros_scope_probe() {
    local p
    p="$(nros_scope_normalize "${1:?nros_scope_probe: platform}")"
    nros_scope_has_module_doctor "$p" || return 0
    just "$p" doctor >/dev/null 2>&1
}

# The provisioned platforms, one per line. ~2 s for the twelve (measured
# 2026-08-31: px4 973 ms and esp32 382 ms dominate; the rest are under 100 ms).
nros_scope_provisioned() {
    local p
    for p in $_NROS_SCOPE_PLATFORMS; do
        if nros_scope_probe "$p"; then
            echo "$p"
        fi
    done
}

# The coverage line every scoped verb prints — phase-411's acceptance item
# "scope, what ran, what did not, and how to provision the rest".
#
# `<verb>` only shapes the wording. With no scope tokens it PROBES and reports
# the derived default; with them it reports the resolution, so a run always
# says what it was about before it starts costing time.
nros_scope_report() {
    local verb="${1:?nros_scope_report: verb}"
    shift
    if [ "$#" -gt 0 ]; then
        local tok
        echo "scope: $* (named — naming it IS the specification)"
        for tok in "$@"; do
            echo "  $tok -> $(nros_scope_kind "$tok")"
        done
        return 0
    fi
    local have missing p
    have=""
    missing=""
    for p in $_NROS_SCOPE_PLATFORMS; do
        if nros_scope_probe "$p"; then
            have="$have $p"
        else
            missing="$missing $p"
        fi
    done
    echo "scope: default — DERIVED by probing, never recorded"
    echo "  provisioned  :${have:- (none)}"
    if [ -n "$missing" ]; then
        echo "  unprovisioned:$missing"
        echo "  ^ these will SKIP. Provision one:  just setup <name>"
    fi
    echo "  Name a scope to require it:  just $verb <scope…>"
}
