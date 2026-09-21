#!/usr/bin/env bash
# Compile what `nros new` emits, instead of asserting strings about it.
#
# Issue 1058 / phase-452 W2.
#
# `cargo-nano-ros/tests/integration_tests.rs` verifies every scaffold variant by
# substring match — `assert!(hpp.contains("::nros::Result configure(...)"))` and
# ~30 siblings. Nothing compiles the result. So the suite answers "did we emit
# the string we meant to emit", which is a question about the template, not
# about the scaffold, and a user's first build is the first compile the emitted
# code ever gets.
#
# The issue's own evidence: phase-417 W-B3 renamed the C++ template's four type
# names to `::rclcpp::` and three of the four did not exist. The template
# compiled in NO configuration and the suite went green, because the only test
# that noticed was asserting the OLD string — so it read as "the rename is
# incomplete" rather than "the rename is wrong".
#
# ## Outside the checkout, deliberately
#
# Measured: scaffolding into `<repo>/tmp/` and configuring there makes
# `find_package(nano_ros)` walk UP and resolve the repository root's own
# `CMakeLists.txt`, which fails on the CLI ownership guard and never reaches
# the question this asks. A user scaffolds somewhere else, so this does too —
# the same reason `check-template-copy-out.sh` copies a template out.
#
# ## Variants
#
# DISCOVERED twice over, so neither list can drift:
#
#   * the LANGUAGES come from `nros new --help`'s own `[possible values: ...]`
#     for `--lang`, not from a list written here;
#   * the BUILD ROAD comes from what the scaffold EMITS — a `Cargo.toml` is
#     built with cargo, a `CMakeLists.txt` with cmake. A fourth language that
#     emits one of those needs no arm here.
#
# Both modes are covered: `--component` (a library node, platform-agnostic) and
# project mode (`--platform native`), because they emit different files.
#
# ## What counts as built
#
# The SCAFFOLD'S OWN source must have compiled — not "the build exited 0", and
# not "some artifact exists". Measured on the C++ component: a successful build
# leaves 9 `.a` files and 8 of them are nano-ros's own dependencies, which
# build whatever the template says. The predicate is the object file for the
# emitted source, or a binary/library named after the emitted package.
#
# `--self-test` is the negative control: it injects the exact undeclared type
# the issue found (`::rclcpp::Result`) into the emitted source and requires a
# FAIL. The tree's own copy of that defect is already fixed — `rclcpp::Result`
# and `rclcpp::Timer` are gone from the template and `rclcpp::Node` became the
# node type's real home in RFC-0089 — so re-breaking a copy is the only form
# "a red before a fix" still has, and a checker never seen to fail is not
# evidence.
#
# Usage: check-scaffold-builds.sh [--list | --self-test] [<lang>_<mode> ...]
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root" || exit 2

# shellcheck source=scripts/lib/grep-q.sh
. "$repo_root/scripts/lib/grep-q.sh"

nros_bin() {
    if [ -n "${NROS_CLI:-}" ] && [ -x "${NROS_CLI}" ]; then
        printf '%s' "$NROS_CLI"
    elif [ -x "$repo_root/packages/cli/target/release/nros" ]; then
        printf '%s' "$repo_root/packages/cli/target/release/nros"
    else
        command -v nros
    fi
}

# The `--lang` values the CLI itself declares, one per line.
discover_langs() {
    local nros
    nros="$(nros_bin)" || return 1
    [ -n "$nros" ] || return 1
    "$nros" new --help 2>&1 \
        | sed -n '/--lang <LANG>/,/--use-case/p' \
        | sed -n 's/.*\[possible values: \([^]]*\)\].*/\1/p' \
        | tr ',' '\n' \
        | tr -d ' ' \
        | sed '/^$/d'
}

variants() {
    local lang
    while IFS= read -r lang; do
        [ -n "$lang" ] || continue
        printf '%s_component\n%s_project\n' "$lang" "$lang"
    done < <(discover_langs)
}

# Emit one variant into <parent>/<name>. Echoes nothing; returns non-zero on
# failure with the log left behind.
scaffold_one() {
    local variant="$1" parent="$2" log="$3" nros lang mode rc
    nros="$(nros_bin)" || return 3
    [ -n "$nros" ] || return 3
    lang="${variant%_*}"
    mode="${variant##*_}"
    rc=0
    if [ "$mode" = "component" ]; then
        ( cd "$parent" && "$nros" new "$variant" --component --lang "$lang" ) > "$log" 2>&1 || rc=$?
    else
        ( cd "$parent" && "$nros" new "$variant" --platform native --lang "$lang" ) > "$log" 2>&1 || rc=$?
    fi
    return "$rc"
}

# Build road, DISCOVERED from what the scaffold emitted.
road_of() {
    local dir="$1"
    if [ -f "$dir/CMakeLists.txt" ]; then printf 'cmake'
    elif [ -f "$dir/Cargo.toml" ]; then printf 'cargo'
    else printf 'unknown'
    fi
}

build_one() {
    local dir="$1" log="$2" road rc
    road="$(road_of "$dir")"
    rc=0
    case "$road" in
        cmake)
            cmake -S "$dir" -B "$dir/build" -DCMAKE_BUILD_TYPE=Release > "$log" 2>&1 || rc=$?
            [ "$rc" -eq 0 ] || return 1
            cmake --build "$dir/build" -j"$(nproc 2>/dev/null || echo 4)" >> "$log" 2>&1 || rc=$?
            ;;
        cargo)
            # The scaffold's OWN documented flow, not a bare `cargo build`.
            # Its emitted `Cargo.toml` says so: "nano-ros crates are not
            # published to crates.io (RFC-0040) -- `version = "*"` is only the
            # patched left-hand side. Run `nros sync` (with NROS_REPO_DIR set)
            # to write the nros-managed [patch.crates-io] block ... then
            # `cargo build`." Skipping it reports `no matching package named
            # \`nros\` found` against crates.io, which reads as the #378 defect
            # and is really this harness omitting a step.
            ( cd "$dir" && NROS_REPO_DIR="$repo_root" "$(nros_bin)" sync ) >> "$log" 2>&1 || rc=$?
            [ "$rc" -eq 0 ] || return 1
            ( cd "$dir" && cargo build ) >> "$log" 2>&1 || rc=$?
            ;;
        *)
            echo "no build road: neither CMakeLists.txt nor Cargo.toml" > "$log"
            return 2
            ;;
    esac
    [ "$rc" -eq 0 ] || return 1
    return 0
}

# Did the scaffold's OWN source compile? Prints what it found.
own_artifacts() {
    local dir="$1" name
    name="$(basename "$dir")"
    # Keyed on the files the SCAFFOLD ITSELF emitted under `src/`, never on
    # "an object exists". The first spelling matched any `*.c.o` under
    # `CMakeFiles/`, which readmitted exactly the vacuity it was written to
    # remove: `c_project` reported 40 "own" artifacts, the example being
    # `builtin_interfaces__nano_ros_c.dir/.../builtin_interfaces_msg_duration.c.o`
    # -- a GENERATED message TU that compiles whatever the template says.
    local src rel
    {
        # A GLOB over the scaffold's own `src/`, not a `find` walk: the set is
        # one directory the scaffold just wrote, and `check-no-tracked-file-find`
        # is right that `find` is the wrong instrument for an enumerable set.
        for src in "$dir"/src/*.c "$dir"/src/*.cpp "$dir"/src/*.rs; do
            [ -f "$src" ] || continue
            rel="$(basename "$src")"
            # cmake writes `<target>.dir/<path-to-source>.o`
            find "$dir/build" -path '*CMakeFiles*' -name "$rel.o" 2>/dev/null
        done
        # a library or binary named after the emitted package
        find "$dir/build" -maxdepth 2 \( -name "lib${name}*.a" -o -name "$name" \) \
             -not -path '*CMakeFiles*' 2>/dev/null
        # cargo: the emitted crate's own binary or rlib
        find "$dir/target" -maxdepth 3 \( -name "$name" -o -name "lib${name}.rlib" \) \
             -type f 2>/dev/null
    } | sed '/^$/d' | sort -u
}

run_one() {
    local variant="$1" work parent dir arts n
    work="$(mktemp -d "${TMPDIR:-/tmp}/nros-scaffold-builds.XXXXXX")" || return 3
    parent="$work"
    dir="$parent/$variant"

    if ! scaffold_one "$variant" "$parent" "$work/new.log"; then
        if [ ! -s "$work/new.log" ] || ! [ -d "$dir" ]; then
            echo "  $variant: FAIL — \`nros new\` did not emit it" >&2
            sed -n '1,8p' "$work/new.log" 2>/dev/null | sed 's/^/      /' >&2
            echo "      full log: $work/new.log" >&2
            return 1
        fi
    fi

    if ! build_one "$dir" "$work/build.log"; then
        echo "  $variant: FAIL — the emitted project does not build" >&2
        grep -n -i -m6 'error' "$work/build.log" 2>/dev/null | sed 's/^/      /' >&2
        echo "      full log: $work/build.log" >&2
        return 1
    fi

    arts="$(own_artifacts "$dir")"
    n="$(printf '%s' "$arts" | grep -c . )"
    if [ "$n" -eq 0 ]; then
        echo "  $variant: FAIL — the build exited 0 but compiled none of the emitted source" >&2
        echo "      (a successful build here leaves ~9 libraries, 8 of them nano-ros's" >&2
        echo "       own deps, so \"an artifact exists\" is the vacuous pass this" >&2
        echo "       predicate refuses; full log: $work/build.log)" >&2
        return 1
    fi
    echo "  $variant: OK — $n own artifact(s), e.g. $(printf '%s' "${arts%%$'\n'*}" | sed "s|$dir/||")"
    rm -rf "$work"
    return 0
}

self_test() {
    # Negative control. Break the emitted source the way phase-417 W-B3 did —
    # an undeclared `::rclcpp::Result` — and require the build to FAIL.
    local work parent variant dir src rc
    work="$(mktemp -d "${TMPDIR:-/tmp}/nros-scaffold-selftest.XXXXXX")" || return 2
    parent="$work"
    variant="cpp_component"
    dir="$parent/$variant"
    if ! scaffold_one "$variant" "$parent" "$work/new.log"; then
        echo "self-test: could not scaffold $variant" >&2
        sed -n '1,6p' "$work/new.log" 2>/dev/null >&2
        return 2
    fi
    src="$(find "$dir/include" -name '*.hpp' | head -1)"
    if [ -z "$src" ]; then
        echo "self-test: $variant emitted no header to break" >&2
        return 2
    fi
    # A SYNTHETIC absent type, not one of the three the issue names.
    #
    # Measured 2026-09-21: all three resolve now. `rclcpp::Result` is
    # `using ::nros::Result;` in `result.hpp`, `rclcpp::Timer` is
    # `using Timer = ::nros::Timer;` at `timer.hpp:268`, and `rclcpp::Node`
    # became the node type's own home in RFC-0089 / phase-427 W7. phase-417's
    # rename was finished correctly after issue 1058 was filed, so there is no
    # historical red left to reproduce — breaking with any of them builds
    # clean, and this control reported exactly that twice before the names were
    # checked.
    #
    # So the control synthesises a name that cannot quietly become real, which
    # proves the checker can fail without pretending to reproduce a defect the
    # tree no longer has.
    sed -i 's|::nros::Timer timer_;|::rclcpp::NrosScaffoldSelfTestAbsentType timer_;|' "$src"
    nros_grep_q -F 'NrosScaffoldSelfTestAbsentType' "$src"
    if [ $? -ne 0 ]; then
        echo "self-test: the emitted header no longer declares \`::nros::Timer timer_;\`," >&2
        echo "  so this control broke nothing. Re-derive the break before trusting a green." >&2
        return 2
    fi

    rc=0
    build_one "$dir" "$work/build.log" || rc=$?
    rm -rf "$work"
    if [ "$rc" -eq 0 ]; then
        echo "self-test FAILED: a scaffold naming an undeclared type BUILT." >&2
        echo "  This checker cannot fail, so its green says nothing." >&2
        return 1
    fi
    echo "self-test OK: an undeclared type in the emitted source is caught."
    return 0
}

mode="run"
case "${1:-}" in
    --list) mode="list"; shift ;;
    --self-test) mode="self-test"; shift ;;
esac

if [ "$mode" = "self-test" ]; then
    self_test
    exit $?
fi

mapfile -t all < <(variants)
if [ "${#all[@]}" -eq 0 ]; then
    echo "check-scaffold-builds: could not read \`nros new --help\`'s --lang values." >&2
    echo "  That is not a pass — an empty variant set means discovery broke, or" >&2
    echo "  there is no \`nros\` CLI here. Build it: just setup-cli" >&2
    exit 1
fi

wanted=("$@")
selected=()
for v in "${all[@]}"; do
    if [ "${#wanted[@]}" -gt 0 ]; then
        hit=0
        for w in "${wanted[@]}"; do [ "$w" = "$v" ] && { hit=1; break; }; done
        [ "$hit" -eq 1 ] || continue
    fi
    selected+=("$v")
done

if [ "$mode" = "list" ]; then
    echo "scaffold variants (languages from \`nros new --help\`, modes component + project):"
    printf '  %s\n' "${selected[@]}"
    exit 0
fi

echo "check-scaffold-builds: compiling ${#selected[@]} scaffold variant(s) outside the checkout"
fail=0
for v in "${selected[@]}"; do
    run_one "$v" || fail=1
done

if [ "$fail" -ne 0 ]; then
    echo "check-scaffold-builds: FAILED — \`nros new\` emits something that does not compile." >&2
    exit 1
fi
echo "check-scaffold-builds: OK"
