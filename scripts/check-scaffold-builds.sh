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
# ## Where a cargo scaffold's own binary lands — READ, never guessed (issue 1446)
#
# A cargo scaffold has TWO layouts and this predicate saw only one, so the one
# variant that built fine was the one it called a vacuous pass:
#
# | variant | artifact, measured 2026-09-22 |
# | --- | --- |
# | `rust_component` (no `system.toml`) | `target/debug/librust_component.rlib` |
# | `rust_project` (`[image.native]`) | `build/native/target/debug/rust_project` |
#
# The second is RFC-0098 D1: a leaf that states a board gets one generated
# settings file per image, `build/<image>/nros-cargo.toml`, carrying a per-image
# `[build] target-dir`; `nros sync` wires it into the leaf's own (gitignored)
# `.cargo/config.toml` as an `include` (issue 1381), so a bare `cargo build`
# inside the leaf honours it. The old predicate searched `$dir/target`
# (absent here) and `$dir/build` at `-maxdepth 2` (the binary is four levels
# down), and reported "the build exited 0 but compiled none of the emitted
# source" about a build whose log says `Compiling rust_project` / `Finished`.
#
# The fix is NOT a wider `-maxdepth` and NOT a recursive `find`. Either one
# readmits the vacuity the predicate exists to refuse — every dependency's
# binary, every `CMakeFiles/*.o` of a generated message TU — and converts a
# false red into a false green, which is strictly worse. Instead:
#
#   * the target dir is READ OUT of the generated settings file, so there is no
#     second derivation of it here to drift from the CLI's (a relative value
#     resolves against the file's GRANDPARENT — cargo's rule for a `--config`
#     file, measured on cargo 1.98.1 and documented in RFC-0098 D1);
#   * the candidates are an ENUMERATED set of exact paths under it — profile
#     dir, optional triple dir, the package's own name — never a walk;
#   * every candidate must be NEWER than a stamp taken immediately before the
#     build, so a leftover artifact cannot answer for a build that produced
#     nothing. `run_one` scaffolds into a fresh `mktemp -d`, so nothing stale
#     can be there in the normal path — the requirement is what makes that
#     structural fact CHECKED instead of assumed, and it is what the locator
#     self-test plants against.
#
# `--self-test` is the negative control, in two groups.
#
# First, the LOCATOR controls (no compiler, no CLI, no network; also reachable
# alone as `--locator-self-test`): each layout in the table above is planted by
# NAME and must be FOUND, and four shapes that must NOT be found are planted
# too — nothing at all, only a stale artifact, only another package's binary in
# the right place, and no pre-build stamp to measure freshness against. This is
# the test issue 1446 asked for: the next move of a scaffold's output is a
# failing control rather than a silent miss.
#
# Then the COMPILE control: it injects an undeclared type into the emitted
# source and requires a FAIL. The tree's own copy of that defect is already
# fixed — `rclcpp::Result` and `rclcpp::Timer` are gone from the template and
# `rclcpp::Node` became the node type's real home in RFC-0089 — so re-breaking
# a copy is the only form "a red before a fix" still has, and a checker never
# seen to fail is not evidence.
#
# Usage: check-scaffold-builds.sh [--list | --self-test | --locator-self-test]
#                                 [<lang>_<mode> ...]
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

# Every cargo target directory this scaffold could have written into, absolute.
#
# READ from the generated settings file rather than re-derived here: RFC-0098 D1
# says a leaf that states a board gets `build/<image>/nros-cargo.toml` with a
# per-image `[build] target-dir`, and that file is the CLI's own statement of
# where the output went. If the CLI ever moves it, this follows, and the gate's
# verdict stays about the scaffold instead of about this script (issue 1446).
#
# A relative value resolves against the file's GRANDPARENT, not its own
# directory — cargo's rule for a `--config` file, measured on cargo 1.98.1 and
# the same rule `cargo_config::base_dir` encodes on the producing side.
#
# Plus cargo's default, `<dir>/target`, for a scaffold with no image at all
# (`--component` emits no `system.toml`, so it gets no settings file).
cargo_target_dirs() {
    local dir="$1" cfg base td
    for cfg in "$dir"/build/*/nros-cargo.toml; do
        [ -f "$cfg" ] || continue
        base="$(cd "$(dirname "$cfg")/.." && pwd -P)" || continue
        td="$(sed -n 's/^[[:space:]]*target-dir[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' \
              "$cfg" | head -1)"
        [ -n "$td" ] || continue
        case "$td" in
            /*) printf '%s\n' "$td" ;;
            *)  printf '%s\n' "$base/$td" ;;
        esac
    done
    printf '%s\n' "$dir/target"
}

# Did the scaffold's OWN source compile? Prints what it found.
#
# `stamp` is a file whose mtime predates the build; every artifact must be
# newer than it, so a leftover from an earlier run can never answer for a build
# that produced nothing. It is REQUIRED — a missing one prints nothing, which
# the caller reports as zero artifacts, because a lenient arm here is the
# vacuous pass this whole predicate exists to refuse.
own_artifacts() {
    local dir="$1" stamp="${2:-}" name
    name="$(basename "$dir")"
    if [ -z "$stamp" ] || [ ! -f "$stamp" ]; then
        echo "own_artifacts: no pre-build stamp, so nothing can be shown to be" >&2
        echo "  a product of THIS build. Refusing to report any artifact." >&2
        return 0
    fi
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
            find "$dir/build" -path '*CMakeFiles*' -name "$rel.o" -newer "$stamp" 2>/dev/null
        done
        # a library or binary named after the emitted package
        find "$dir/build" -maxdepth 2 \( -name "lib${name}*.a" -o -name "$name" \) \
             -not -path '*CMakeFiles*' -newer "$stamp" 2>/dev/null
        # cargo: the emitted crate's own binary or rlib, at an ENUMERATED path
        # under a target dir the generated settings NAME (see above). Never a
        # walk: the shapes are `<td>/<profile>/<artifact>` and, for a board with
        # a rustc triple, `<td>/<triple>/<profile>/<artifact>`.
        local td prof cand
        while IFS= read -r td; do
            [ -d "$td" ] || continue
            for prof in debug release; do
                for cand in \
                    "$td/$prof/$name" "$td/$prof/lib$name.rlib" \
                    "$td"/*/"$prof/$name" "$td"/*/"$prof/lib$name.rlib"; do
                    [ -f "$cand" ] || continue
                    [ "$cand" -nt "$stamp" ] || continue
                    printf '%s\n' "$cand"
                done
            done
        done < <(cargo_target_dirs "$dir")
    } | sed '/^$/d' | sort -u
}

run_one() {
    local variant="$1" work parent dir arts n stamp td
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

    # Everything the build writes is newer than this, and nothing that was here
    # before it is. Backdated one second so a filesystem with coarse mtimes
    # cannot tie: the slack can readmit nothing, because the only writer before
    # this point is `nros new` itself, moments earlier, into a `mktemp -d`.
    stamp="$work/.pre-build"
    : > "$stamp"
    touch -d '1 second ago' "$stamp" 2>/dev/null || true

    if ! build_one "$dir" "$work/build.log"; then
        echo "  $variant: FAIL — the emitted project does not build" >&2
        grep -n -i -m6 'error' "$work/build.log" 2>/dev/null | sed 's/^/      /' >&2
        echo "      full log: $work/build.log" >&2
        return 1
    fi

    arts="$(own_artifacts "$dir" "$stamp")"
    # `nros_grep_count`, not `… | grep -c`: an ERRORING grep prints nothing and
    # exits 2, `n` comes back EMPTY, and `[ "$n" -eq 0 ]` then returns 2 — so
    # bash skips the FAIL branch and falls through to the OK line with a blank
    # count. A tool failure reported as a pass is the same vacuity this
    # predicate exists to refuse, one layer up (issue 0726's class).
    nros_grep_count n . <<<"$arts"
    if [ "$n" -eq 0 ]; then
        echo "  $variant: FAIL — the build exited 0 and left no artifact of the emitted source" >&2
        echo "      (a successful build here leaves ~9 libraries, 8 of them nano-ros's" >&2
        echo "       own deps, so \"an artifact exists\" is the vacuous pass this" >&2
        echo "       predicate refuses)" >&2
        # WHERE it looked, because a locator that cannot see the artifact and a
        # build that did not produce one read identically otherwise — which is
        # what issue 1446 cost. Say it, so the next reader can tell them apart.
        echo "      looked for \`$variant\` / \`lib$variant.{rlib,a}\` under, newer than the build:" >&2
        while IFS= read -r td; do
            echo "        $td/{debug,release}[/<triple>]  $([ -d "$td" ] && echo '(exists)' || echo '(absent)')" >&2
        done < <(cargo_target_dirs "$dir")
        echo "        $dir/build  (cmake, depth 2, plus CMakeFiles/*.o for src/*)" >&2
        echo "      full log: $work/build.log" >&2
        return 1
    fi
    echo "  $variant: OK — $n own artifact(s), e.g. $(printf '%s' "${arts%%$'\n'*}" | sed "s|$dir/||")"
    rm -rf "$work"
    return 0
}

# Controls for the LOCATOR, issue 1446. No compiler, no CLI, no network: each
# layout is planted by NAME and the predicate must answer the way the layout
# says. SEVEN cases, four of them refusals — because widening a locator is
# exactly the change that can turn a false red into a false green, and a green
# that cannot fail is what this file already says is not evidence.
#
#   1  RFC-0098 D1 project layout   `build/<img>/target/<prof>/<name>`  FOUND
#   2  component layout             `target/<prof>/lib<name>.rlib`      FOUND
#   3  nothing built                                                    REFUSED
#   4  only a STALE artifact (older than the build)                     REFUSED
#   5  only ANOTHER package's binary in the right directory             REFUSED
#   6  no pre-build stamp, so freshness cannot be established           REFUSED
#   7  a NON-DEFAULT `target-dir` in the settings file                  FOUND
#
# Case 4 is the hazard the widening introduces and the reason this fix is not a
# `-maxdepth` bump: `run_one` builds in a fresh `mktemp -d`, so nothing stale
# can be there — this is what makes that structural fact CHECKED. Case 5 is the
# vacuity the original predicate was written to refuse, re-asserted against the
# new arm. Case 7 is what separates READING the layout from guessing it.
#
# Each is mutation-tested: reverting the cargo arm fails 1, the naive recursive
# `find` fails 4, hardcoding `build/*/target` fails 7, and matching any file in
# the profile directory fails 5.
locator_self_test() {
    local work dir name td stamp got rc fails=0
    work="$(mktemp -d "${TMPDIR:-/tmp}/nros-scaffold-locator.XXXXXX")" || return 2
    name="rust_project"

    # A plant is a whole scaffold shape: the package dir, the generated settings
    # file that NAMES the target dir (so the locator reads it, as it does in
    # anger), and whatever artifact the case is about. `image` empty plants the
    # COMPONENT shape — no `system.toml`, so no settings file at all.
    _plant() {
        local case_dir="$1" image="${2:-}"
        dir="$case_dir/$name"
        mkdir -p "$dir/src"
        : > "$dir/src/main.rs"
        if [ -n "$image" ]; then
            mkdir -p "$dir/build/$image"
            printf '[build]\ntarget-dir = "%s/target"\n' "$image" \
                > "$dir/build/$image/nros-cargo.toml"
        fi
        stamp="$case_dir/.pre-build"
        : > "$stamp"
        touch -d '2024-01-01 00:00:00' "$stamp"
    }
    _expect() {
        local label="$1" want="$2" pattern="${3:-}" hit
        got="$(own_artifacts "$dir" "$stamp" 2>/dev/null)"
        case "$want" in
            found)
                hit=0
                if [ -n "$pattern" ]; then
                    # Here-string, never a pipe: issue 1077.
                    nros_grep_q -F -- "$pattern" <<<"$got" || hit=$?
                fi
                if [ -z "$got" ]; then
                    echo "self-test FAILED ($label): the locator found NOTHING." >&2
                    echo "  This layout is what a scaffold writes; a gate that cannot see" >&2
                    echo "  it reports a fine build as a vacuous pass (issue 1446)." >&2
                    fails=1
                elif [ "$hit" -ne 0 ]; then
                    echo "self-test FAILED ($label): found something else than \`$pattern\`:" >&2
                    printf '%s\n' "$got" | sed 's/^/    /' >&2
                    fails=1
                else
                    echo "  locator control OK ($label): $(printf '%s' "${got%%$'\n'*}" | sed "s|$dir/||")"
                fi
                ;;
            refused)
                if [ -n "$got" ]; then
                    echo "self-test FAILED ($label): the locator accepted:" >&2
                    printf '%s\n' "$got" | sed 's/^/    /' >&2
                    echo "  A false GREEN is worse than the false red this fix removed." >&2
                    fails=1
                else
                    echo "  locator control OK ($label): refused, as it must"
                fi
                ;;
        esac
    }

    # 1 — the RFC-0098 D1 layout, by name.
    _plant "$work/c1" native
    mkdir -p "$dir/build/native/target/debug"
    : > "$dir/build/native/target/debug/$name"
    _expect "RFC-0098 project layout" found "build/native/target/debug/$name"

    # 2 — the component layout (no image, so cargo's default `target/`).
    _plant "$work/c2"
    mkdir -p "$dir/target/debug"
    : > "$dir/target/debug/lib$name.rlib"
    _expect "component target/ layout" found "target/debug/lib$name.rlib"

    # 3 — a build that produced nothing at all.
    _plant "$work/c3" native
    mkdir -p "$dir/build/native/target/debug"
    _expect "nothing built" refused

    # 4 — only a STALE artifact: right path, older than the build.
    _plant "$work/c4" native
    mkdir -p "$dir/build/native/target/debug"
    : > "$dir/build/native/target/debug/$name"
    touch -d '2020-01-01 00:00:00' "$dir/build/native/target/debug/$name"
    _expect "stale artifact only" refused

    # 5 — only another package's binary, in the right directory.
    _plant "$work/c5" native
    mkdir -p "$dir/build/native/target/debug"
    : > "$dir/build/native/target/debug/some_other_pkg"
    : > "$dir/build/native/target/debug/libnros_core.rlib"
    _expect "another package's artifact only" refused

    # 6 — the stamp itself is the precondition: without one, nothing may pass.
    _plant "$work/c6" native
    mkdir -p "$dir/build/native/target/debug"
    : > "$dir/build/native/target/debug/$name"
    stamp=""
    _expect "no pre-build stamp" refused

    unset -f _plant _expect
    rm -rf "$work"
    [ "$fails" -eq 0 ] || return 1
    # And prove the reader can be read: the settings file is where the target
    # dir comes from, so a value that is NOT the default must be honoured.
    work="$(mktemp -d "${TMPDIR:-/tmp}/nros-scaffold-locator.XXXXXX")" || return 2
    dir="$work/$name"
    mkdir -p "$dir/src" "$dir/build/odd" "$dir/build/odd/somewhere/else/debug"
    : > "$dir/src/main.rs"
    printf '[build]\ntarget-dir = "odd/somewhere/else"\n' > "$dir/build/odd/nros-cargo.toml"
    : > "$dir/build/odd/somewhere/else/debug/$name"
    stamp="$work/.pre-build"
    : > "$stamp"
    touch -d '2024-01-01 00:00:00' "$stamp"
    got="$(own_artifacts "$dir" "$stamp" 2>/dev/null)"
    rm -rf "$work"
    rc=0
    nros_grep_q -F -- 'odd/somewhere/else/debug' <<<"$got" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "self-test FAILED (settings file is read): a non-default \`target-dir\`" >&2
        echo "  was not honoured, so the locator is guessing the path instead of" >&2
        echo "  reading the CLI's own statement of it (issue 1446)." >&2
        return 1
    fi
    echo "  locator control OK (settings file is read, not guessed)"
    return 0
}

self_test() {
    # Negative control. Break the emitted source the way phase-417 W-B3 did —
    # an undeclared `::rclcpp::Result` — and require the build to FAIL.
    local work parent variant dir src rc

    locator_self_test || return $?
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
    # The locator controls alone: no CLI, no compiler, no network, so they run
    # anywhere and answer in milliseconds. `--self-test` runs them too.
    --locator-self-test) mode="locator-self-test"; shift ;;
esac

if [ "$mode" = "locator-self-test" ]; then
    locator_self_test
    exit $?
fi

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
