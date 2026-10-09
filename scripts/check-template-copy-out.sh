#!/usr/bin/env bash
# Build every copy-out template the way a user gets it — by copying it out.
#
# Issue 1108 / phase-452 W1.
#
# `examples/templates/` is the one part of the tree whose contents are COPIED by
# a stranger. Being wrong there is replicated rather than merely observed, and
# until now nothing built them: `check-example-cargo-dirs` and
# `check-workspace-root-build-files` ask static questions about the files, and a
# template can satisfy both while `nros build` refuses it outright.
#
# It was not hypothetical. The first run of this script found
# `multi-package-workspace` declaring `<depend>nano-ros</depend>` in two
# package.xml files — a name that resolves to nothing (`[prereq.nros]` is the
# key, and `nano-ros` with a hyphen is not even a legal ROS package name), so
# `nros build --workspace` failed at dependency resolution. Two static gates and
# a colcon-parity job had been green over it since the template was written.
#
# ## Copying out is the point
#
# A template built IN PLACE is not the thing a user gets. In place it can reach
# the repo root's `Cargo.toml`, a `.cargo/config.toml` above it, a sibling's
# `generated/`, or a file nobody committed. So the copy is made from
# `git ls-files` — the TRACKED set, which is what a clone hands over — with
# WORKTREE content, so a contributor editing a template tests the edit rather
# than HEAD. An untracked file a template has come to depend on therefore shows
# up here as a build failure, which is the report we want.
#
# ## Which templates — and which PROJECTS inside them
#
# DISCOVERED, never listed. A template is buildable iff some tracked
# `system.toml` under it declares an `[image.<id>]` — that is exactly the
# question `nros build` asks (RFC-0098 D3), so this cannot drift from it.
# Templates that declare none are REPORTED as skipped with that reason, never
# silently dropped.
#
# The unit that gets BUILT is the PROJECT, not the template (issue 1764). A
# workspace template's bringups sit under `src/` and belong to the template
# root, which `nros build --workspace` discovers. A template can also hold
# self-contained SUB-PROJECTS: phase-482 W3 gave `cpp-port-minimal-publisher`
# `mps2-an385-freertos/` and `zephyr/`, each a leaf with its own
# `CMakeLists.txt` and `system.toml`, and no `system.toml` at the root. Building
# the root then failed with "declares no `[image.*]`". The first repair counted
# only `board = "native"` images, which turned the gate green by checking
# neither sub-project. The rule is now one function,
# `scripts/lib/template_projects.py`: a `system.toml` under a `src/` component
# belongs to the directory above it, anything else to its own directory.
#
# The whole template is still what gets copied — a sub-project reaches
# `../src/minimal_publisher.cpp`, which is exactly the kind of dependency a copy
# has to carry — and the build runs in the project's directory INSIDE the copy.
#
# ## Host and cross projects
#
# A project whose images are all `board = "native"` is built the way a user
# builds a workspace: `nros sync` + `nros build --workspace`.
#
# A CROSS project is built by the road its board DECLARES, every parameter read
# from data rather than spelled here:
#
# * `nros ws board-facts` resolves the board's descriptor and the SDK roots the
#   leaf's own `[board_config.*] sdk` names (`{env:FREERTOS_DIR}`).
# * A descriptor with `[board.cmake] toolchain_file` is a CMake leaf: the leaf's
#   own `CMakeLists.txt`, configured with that toolchain — the C/C++ road the
#   getting-started pages document. (`nros build` in a single-package CMake leaf
#   is issues 1296/1308, so it is not the road here.)
# * A Zephyr leaf has NO road this gate can derive: its west board and Kconfig
#   fragments are not in its data (issue 1782), so it is reported NOT VERIFIED
#   with that reason even where Zephyr is installed.
# * Any other board is a FAIL naming it, so a new kind of sub-project forces a
#   decision here instead of being skipped by default.
#
# A missing precondition — an SDK root that is unset or empty, a cross compiler
# the toolchain file cannot resolve, no Zephyr workspace — is a NAMED skip
# through `nros_check_unverified`: the `nros_check_skip` ledger, so the lane's
# closing line lists it, and a FAIL under `NROS_CHECK_SKIP_STRICT=1`. Never a
# pass: a sub-project nobody built has verified nothing.
#
# ## What counts as built
#
# rc=0 alone is vacuous — a builder that configures and links nothing also
# exits 0. The artifact predicate is: at least one ELF executable under the
# copy's `build/` that is not a CMake compiler probe, a cargo build script, a
# dep/incremental artifact or a metadata probe. Measured, the templates that
# build produce THREE different spellings —
# `build/<deploy>/cmake/<image>_entry`, `build/<deploy>/cmake/pkg/<pkg>/<pkg>`
# and `build/<deploy>/<image>_entry/target/debug/<image>_entry` — and a
# template that fails produces none. Three spellings, one predicate, which is
# why the predicate asks "is it an ELF this build made" rather than naming a
# path: a fourth road needs no arm here.
#
# `--self-test` is the negative control: it breaks a copy the same way the real
# defect did and requires this script to report FAIL. A checker that has never
# been seen to fail is not evidence.
#
# Usage: check-template-copy-out.sh [--list | --self-test] [template-name ...]
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root" || exit 2

# shellcheck source=scripts/lib/grep-q.sh
. "$repo_root/scripts/lib/grep-q.sh"
# shellcheck source=scripts/build/check-skip.sh
. "$repo_root/scripts/build/check-skip.sh"
# shellcheck source=scripts/build/cmake-cache-guard.sh
. "$repo_root/scripts/build/cmake-cache-guard.sh"

templates_dir="examples/templates"
gate="template-copy-out"

# Every tracked template DIRECTORY, one per line. `NF > 3` is what makes it a
# directory rather than a file: `examples/templates/README.md` has three fields
# and is not a template, and reporting it as one made the skip list read as if
# the tree had two more templates than it has.
discover_templates() {
    git ls-files "$templates_dir" \
        | awk -F/ 'NF > 3 { print $3 }' \
        | sort -u
}

# Print `<project>\t<image>\t<board>` for every image a template declares,
# `<project>` relative to the template (`.` for its root). ONE function decides
# which project a `system.toml` belongs to — see the header and issue 1764.
template_images() {
    local tmpl="$1" files rc=0
    # Issue 1249: a status meant to be inspected is captured, never lost in an
    # argument-position `$(…)`.
    files="$(git ls-files "$templates_dir/$tmpl")" || rc=$?
    [ "$rc" -eq 0 ] || return 2
    local -a list=()
    mapfile -t list <<< "$files"
    python3 "$repo_root/scripts/lib/template_projects.py" "$templates_dir/$tmpl" "${list[@]}"
}

# The distinct projects of a template, one per line (`.` sorts first). Returns
# 1 when the template declares no image at all.
template_projects() {
    local tmpl="$1" rows rc=0
    rows="$(template_images "$tmpl")" || rc=$?
    [ "$rc" -eq 0 ] || return 2
    [ -n "$rows" ] || return 1
    printf '%s\n' "$rows" | cut -f1 | sort -u
}

# The boards one project's images name, one per line.
project_boards() {
    local tmpl="$1" proj="$2" rows rc=0
    rows="$(template_images "$tmpl")" || rc=$?
    [ "$rc" -eq 0 ] || return 2
    printf '%s\n' "$rows" | awk -F'\t' -v p="$proj" '$1 == p { print $3 }' | sort -u
}

# `<tmpl>` for the root project, `<tmpl>/<proj>` for a sub-project.
project_label() {
    if [ "$2" = "." ]; then printf '%s' "$1"; else printf '%s/%s' "$1" "$2"; fi
}

# Copy a template's TRACKED files, with worktree content, into <dest>.
copy_out() {
    local tmpl="$1" dest="$2" f
    mkdir -p "$dest" || return 1
    while IFS= read -r f; do
        mkdir -p "$dest/$(dirname "$f")" || return 1
        cp -p "$f" "$dest/$f" || return 1
    done < <(git ls-files "$templates_dir/$tmpl")
}

# True when <file> starts with the ELF magic.
#
# Hex, not `od -c`. The first spelling of this compared against `\0177ELF`,
# and `od -An -c` emits `177ELF` — no backslash — so the predicate could never
# return true and every template would have been reported as "built nothing".
# It survived the negative control because that control breaks a package.xml,
# which fails at the BUILD arm and never reaches this one: the self-test's
# reach was narrower than the rule it was written for. `classifier_self_test`
# below is the arm that covers it.
is_elf() {
    [ "$(head -c 4 "$1" 2>/dev/null | od -An -tx1 | tr -d ' \n')" = "7f454c46" ]
}

# ELF executables the build produced, excluding the toolchain's own scaffolding.
built_artifacts() {
    local ws="$1"
    [ -d "$ws/build" ] || return 0
    find "$ws/build" -type f -executable 2>/dev/null \
        | grep -v -e '/CMakeFiles/' \
                  -e '/build-script-build$' \
                  -e '/build_script_build' \
                  -e '/deps/' \
                  -e '/incremental/' \
                  -e '/nros-metadata/' \
        | while IFS= read -r f; do
              is_elf "$f" && printf '%s\n' "$f"
          done
}

nros_bin() {
    if [ -n "${NROS_CLI:-}" ] && [ -x "${NROS_CLI}" ]; then
        printf '%s' "$NROS_CLI"
    elif [ -x "$repo_root/packages/cli/target/release/nros" ]; then
        printf '%s' "$repo_root/packages/cli/target/release/nros"
    else
        command -v nros
    fi
}


# Build one copied-out HOST project with `nros sync` + `nros build`. Returns 0
# on success, 1 on a build failure, 3 when there is no `nros` to build with.
build_copy() {
    local ws="$1" log="$2" nros rc
    nros="$(nros_bin)" || return 3
    [ -n "$nros" ] || return 3
    : > "$log"
    rc=0
    "$nros" sync "$ws" >> "$log" 2>&1 || rc=$?
    [ "$rc" -eq 0 ] || return 1
    rc=0
    "$nros" build --workspace "$ws" >> "$log" 2>&1 || rc=$?
    [ "$rc" -eq 0 ] || return 1
    return 0
}

# Build one copied-out CROSS project for <board> by the road its descriptor
# declares (see the header). Returns 0 built, 1 failed, 3 no `nros`, and 4 when a
# precondition is absent — in which case the skip is already RECORDED in the
# ledger under <label> — or 5 for the same absence under
# NROS_CHECK_SKIP_STRICT=1, where it is a failure.
build_cross() {
    local proj="$1" board="$2" log="$3" label="$4" nros rc facts err
    nros="$(nros_bin)" || return 3
    [ -n "$nros" ] || return 3
    : > "$log"

    # The board's facts, from the leaf's own system.toml. An SDK root the leaf
    # names through `{env:VAR}` with VAR unset is the CLI's refusal "… which is
    # not set" — that is the SDK being absent, not a template defect.
    err="$(mktemp)" || return 1
    rc=0
    facts="$("$nros" ws board-facts "$proj" --board "$board" 2>"$err")" || rc=$?
    if [ "$rc" -ne 0 ]; then
        cat "$err" >> "$log"
        local unset_rc=0
        nros_grep_q 'which is not set' "$err" || unset_rc=$?
        case "$unset_rc" in
            0)
                local why
                why="$(sed -n '/which is not set/{s/^Error: //;s|^[^ ]*system.toml: ||;p;q;}' "$err")"
                rm -f "$err"
                nros_check_unverified "$gate" "$label ($board): SDK not provisioned — $why" || return 5
                return 4
                ;;
        esac
        rm -f "$err"
        return 1
    fi
    rm -f "$err"

    local board_toml="" platform="" line key val
    local -a sdk_missing=()
    while IFS= read -r line; do
        key="${line%%=*}"
        val="${line#*=}"
        case "$key" in
            NROS_BOARD_TOML) board_toml="$val" ;;
            NROS_PLATFORM_NAME) platform="$val" ;;
            NROS_SDK_*)
                # A root that exists but is EMPTY is an uninitialised submodule,
                # which is how a checkout without that SDK looks.
                if [ -z "$val" ] || [ ! -d "$val" ] || [ -z "$(ls -A "$val" 2>/dev/null)" ]; then
                    sdk_missing+=("${key#NROS_SDK_}=${val:-<empty>}")
                fi
                ;;
        esac
    done <<< "$facts"
    if [ "${#sdk_missing[@]}" -gt 0 ]; then
        nros_check_unverified "$gate" "$label ($board): SDK not provisioned — absent or empty: ${sdk_missing[*]}" || return 5
        return 4
    fi
    [ -n "$board_toml" ] || { echo "board-facts named no descriptor for '$board'" >> "$log"; return 1; }

    local helper="$repo_root/scripts/lib/template_projects.py" toolchain west_board
    toolchain="$(python3 "$helper" descriptor "$board_toml" "$board" cmake.toolchain_file)" || return 1
    west_board="$(python3 "$helper" descriptor "$board_toml" "$board" zephyr.west_board)" || return 1

    if [ -n "$toolchain" ]; then
        local tc="$repo_root/$toolchain" cc
        [ -f "$tc" ] || { echo "toolchain file $toolchain (from $board_toml) does not exist" >> "$log"; return 1; }
        cc="$(nros_cmake_toolchain_resolved_cc "$tc")"
        if [ -z "$cc" ] || ! command -v "$cc" >/dev/null 2>&1; then
            nros_check_unverified "$gate" "$label ($board): cross compiler not provisioned — $toolchain resolves none (nros setup $board)" || return 5
            return 4
        fi
        local -a gen=()
        command -v ninja >/dev/null 2>&1 && gen=(-G Ninja)
        rc=0
        cmake "${gen[@]}" -S "$proj" -B "$proj/build" -DCMAKE_TOOLCHAIN_FILE="$tc" >> "$log" 2>&1 || rc=$?
        [ "$rc" -eq 0 ] || return 1
        cmake --build "$proj/build" >> "$log" 2>&1 || return 1
        return 0
    fi

    if [ "$platform" = "zephyr" ] || [ -n "$west_board" ]; then
        local zws=""
        zws="$(bash "$repo_root/scripts/lib/zephyr-workspace.sh" --absolute resolve 2>/dev/null)" || zws=""
        if [ -z "$zws" ] || [ ! -d "$zws/zephyr" ]; then
            nros_check_unverified "$gate" "$label ($board): Zephyr not provisioned — no west workspace resolves (just zephyr setup)" || return 5
            return 4
        fi
        # Issue 1782: the leaf's data does not carry its whole west build — the
        # generic `zephyr` board lowers to `$west_board`, where the port
        # measured a compile failure, and the board it targets plus its Kconfig
        # fragments are stated only in its README and its fixture row. So even
        # here there is no road to derive, and saying so is the honest verdict.
        nros_check_unverified "$gate" "$label ($board): no copy-out road derivable for a Zephyr leaf (west board '${west_board:-?}' and Kconfig fragments are not in its data — issue 1782)" || return 5
        return 4
    fi

    echo "board '$board' ($board_toml) declares neither [board.cmake] toolchain_file nor [board.zephyr] west_board," >> "$log"
    echo "so this gate has no road for it. Teach scripts/check-template-copy-out.sh one." >> "$log"
    return 1
}

# Report a failed build with the TAIL of its log.
#
# The TAIL, not the head. This printed `sed -n '1,12p'` until issue 1453's
# 2026-09-28 section: the first lines of a copy's build are `nros sync`'s
# progress (`sync: codegen std_msgs`, `sync: resolved …`), so the twelve lines
# shown were always preamble and the error — which cargo, cmake and the CLI's
# own refusals all put LAST — was never among them. Two CI runs reported this
# template as failing with no visible reason, and the reason was in the log the
# whole time, below the cut.
report_build_failure() {
    local label="$1" log="$2"
    echo "  $label: FAIL — the copy does not build" >&2
    if [ -s "$log" ]; then
        echo "      last 40 line(s) of the copy's build log:" >&2
        tail -n 40 "$log" | sed 's/^/      /' >&2
        echo "      full log: $log" >&2
    fi
}

# Build one PROJECT of a template from a copy of the WHOLE template. Returns 0
# OK, 1 FAIL, 2 no `nros` CLI, 4 not verified (recorded in the ledger), 6 a
# precondition absent under NROS_CHECK_SKIP_STRICT=1.
run_one() {
    local tmpl="$1" proj="$2" label work dest log arts n
    label="$(project_label "$tmpl" "$proj")"
    work="$(mktemp -d "${TMPDIR:-/tmp}/nros-template-copy-out.XXXXXX")" || return 3
    dest="$work/copy"
    log="$work/build.log"
    if ! copy_out "$tmpl" "$dest"; then
        echo "  $label: FAIL — could not copy the tracked file set out" >&2
        rm -rf "$work"
        return 1
    fi
    local ws="$dest/$templates_dir/$tmpl" pdir boards b
    if [ "$proj" = "." ]; then pdir="$ws"; else pdir="$ws/$proj"; fi
    boards="$(project_boards "$tmpl" "$proj")" || { echo "  $label: FAIL — could not read its images" >&2; return 1; }

    local brc=0 host=0 cross=()
    while IFS= read -r b; do
        [ -n "$b" ] || continue
        if [ "$b" = "native" ]; then host=1; else cross+=("$b"); fi
    done <<< "$boards"

    if [ "$host" -eq 1 ]; then
        # A workspace's cross images are built by that platform's own lane;
        # this lane builds what `nros build` builds on the host. Recorded, not
        # dropped, so the closing line still says they went unverified here.
        for b in "${cross[@]}"; do
            nros_check_unverified "$gate" "$label ($b): a cross image in a host workspace is not built by this lane" || brc=5
        done
        if [ "$brc" -eq 0 ]; then
            build_copy "$pdir" "$log" || brc=$?
        fi
    else
        local one any_built=0 any_skipped=0
        for b in "${cross[@]}"; do
            one=0
            build_cross "$pdir" "$b" "$log" "$label" || one=$?
            case "$one" in
                0) any_built=1 ;;
                4) any_skipped=1 ;;
                *) brc="$one"; break ;;
            esac
        done
        if [ "$brc" -eq 0 ] && [ "$any_built" -eq 0 ] && [ "$any_skipped" -eq 1 ]; then
            echo "  $label: NOT VERIFIED — see [SKIPPED] above"
            rm -rf "$work"
            return 4
        fi
    fi

    if [ "$brc" -eq 3 ]; then
        # NOT a template failure, and saying so matters: in a pristine worktree
        # this arm is the whole verdict, and the first spelling reported
        # "the copy does not build" over a `sed: can't read .../build.log`,
        # which blames the template for a missing tool.
        echo "  $label: FAIL — no \`nros\` CLI to build with" >&2
        echo "      Looked at \$NROS_CLI, packages/cli/target/release/nros," >&2
        echo "      then PATH. Build it: just setup-cli" >&2
        return 2
    fi
    if [ "$brc" -eq 5 ]; then
        echo "  $label: FAIL — a precondition is absent and NROS_CHECK_SKIP_STRICT=1 (see [FAIL] above)" >&2
        rm -rf "$work"
        return 6
    fi
    if [ "$brc" -ne 0 ]; then
        report_build_failure "$label" "$log"
        return 1
    fi
    arts="$(built_artifacts "$pdir")"
    n="$(printf '%s' "$arts" | grep -c . )"
    if [ "$n" -eq 0 ]; then
        echo "  $label: FAIL — build exited 0 but produced no executable" >&2
        echo "      (rc=0 with an empty artifact set is the vacuous pass this" >&2
        echo "       predicate exists to catch; full log: $log)" >&2
        return 1
    fi
    echo "  $label: OK — $n artifact(s), e.g. ${arts%%$'\n'*}" | sed "s|$pdir/||"
    rm -rf "$work"
    return 0
}

# Positive + negative control for `is_elf`, which no build-stage control reaches.
classifier_self_test() {
    local bin
    bin="$(command -v bash)"
    if ! is_elf "$bin"; then
        echo "self-test FAILED: is_elf says $bin is not an ELF." >&2
        echo "  The artifact predicate cannot succeed, so every template would" >&2
        echo "  be reported as having built nothing." >&2
        return 1
    fi
    if is_elf "${BASH_SOURCE[0]}"; then
        echo "self-test FAILED: is_elf says this shell script IS an ELF." >&2
        return 1
    fi
    echo "self-test OK: is_elf accepts an ELF and rejects a script."
    return 0
}

# Issue 1764's own control: the PROJECT rule. Every sub-project leaf must be its
# own project and every `src/` bringup must belong to the root — asked of the
# helper on synthetic paths (it needs no SDK), then of the real tree: a
# template whose root declares no image must not be reported as a root project.
projects_self_test() {
    python3 "$repo_root/scripts/lib/template_projects.py" --self-test || return 1
    local tmpl projs rc
    for tmpl in $(discover_templates); do
        rc=0
        projs="$(template_projects "$tmpl")" || rc=$?
        [ "$rc" -eq 0 ] || continue
        local src_manifests root_rc=0
        src_manifests="$(git ls-files "$templates_dir/$tmpl/src")"
        nros_grep_q -x '\.' <<<"$projs" || root_rc=$?
        [ "$root_rc" -le 1 ] || return 1
        if [ ! -f "$templates_dir/$tmpl/system.toml" ] \
            && [[ "$src_manifests" != *"/system.toml"* ]] \
            && [ "$root_rc" -eq 0 ]; then
            echo "self-test FAILED: $tmpl has no root or src/ system.toml, yet its ROOT is a project." >&2
            echo "  That is issue 1764: \`nros build\` at that root finds no [image.*]." >&2
            return 1
        fi
    done
    echo "self-test OK: projects are where their system.toml says (no root project without a root image)."
    return 0
}

self_test() {
    # Negative control. Break a copy the way the real defect did — a <depend>
    # that resolves to nothing — and require a FAIL. Uses the first template
    # with a HOST root project and a package.xml, so it cannot be outlived by a
    # rename.
    local tmpl work dest ws pkg rel rc
    # The package.xml comes from the INDEX, not from a walk of the copy: it is
    # a tracked file, and `check-no-tracked-file-find` is right that `find` is
    # the wrong instrument for one (7m36s -> 0.8s over the same paths). The
    # copy mirrors repo-relative paths, so the index path maps straight in.
    rel=""
    for tmpl in $(discover_templates); do
        local root_boards native_rc=0
        root_boards="$(project_boards "$tmpl" "." 2>/dev/null)"
        nros_grep_q -x native <<<"$root_boards" || native_rc=$?
        [ "$native_rc" -le 1 ] || return 2
        [ "$native_rc" -eq 0 ] || continue
        rel="$(git ls-files "$templates_dir/$tmpl" | sed -n '/\/package\.xml$/{p;q;}')"
        [ -n "$rel" ] || continue
        break
    done
    [ -n "${tmpl:-}" ] && [ -n "$rel" ] || {
        echo "self-test: no buildable template with a package.xml to break" >&2
        return 2
    }

    work="$(mktemp -d "${TMPDIR:-/tmp}/nros-template-selftest.XXXXXX")" || return 2
    dest="$work/copy"
    copy_out "$tmpl" "$dest" || { echo "self-test: copy failed" >&2; return 2; }
    ws="$dest/$templates_dir/$tmpl"
    pkg="$dest/$rel"
    # `nano-ros` is the exact name that resolved to nothing; a hyphen also makes
    # it an illegal ROS package name, so no future rung can rescue it.
    sed -i 's|</package>|  <depend>nano-ros</depend>\n</package>|' "$pkg"

    rc=0
    build_copy "$ws" "$work/build.log" || rc=$?
    rm -rf "$work"
    if [ "$rc" -eq 0 ]; then
        echo "self-test FAILED: a template with an unresolvable <depend> BUILT." >&2
        echo "  This checker cannot fail, so its green says nothing." >&2
        return 1
    fi
    echo "self-test OK: an unresolvable <depend> is caught (broke $tmpl)."
    return 0
}

# The cross arm's negative control: a sub-project whose copy cannot compile must
# be a FAIL. Breaks the first CMake sub-project it finds by naming a source
# that does not exist. Where that sub-project's SDK is absent the control
# cannot run, and it says so through the ledger rather than passing.
cross_self_test() {
    local tmpl proj b work dest pdir rc label
    for tmpl in $(discover_templates); do
        while IFS= read -r proj; do
            [ "$proj" != "." ] || continue
            b="$(project_boards "$tmpl" "$proj" | grep -vx native | head -n1)"
            [ -n "$b" ] && [ -f "$templates_dir/$tmpl/$proj/CMakeLists.txt" ] || continue
            label="$(project_label "$tmpl" "$proj")"
            work="$(mktemp -d "${TMPDIR:-/tmp}/nros-template-selftest.XXXXXX")" || return 2
            dest="$work/copy"
            copy_out "$tmpl" "$dest" || { echo "self-test: copy failed" >&2; rm -rf "$work"; return 2; }
            pdir="$dest/$templates_dir/$tmpl/$proj"
            printf '\nadd_executable(nros_copy_out_selftest nros_copy_out_selftest_missing.cpp)\n' \
                >> "$pdir/CMakeLists.txt"
            rc=0
            build_cross "$pdir" "$b" "$work/build.log" "$label [self-test]" || rc=$?
            # A failure only counts when it is the one we caused: a copy that
            # fails for some OTHER reason would pass this control while saying
            # nothing about whether the arm can see a broken sub-project.
            local ours=0 ours_rc=0
            nros_grep_q 'nros_copy_out_selftest_missing' "$work/build.log" || ours_rc=$?
            [ "$ours_rc" -eq 0 ] && ours=1
            rm -rf "$work"
            case "$rc" in
                0)
                    echo "self-test FAILED: $label built with a missing source — the cross arm cannot fail." >&2
                    return 1 ;;
                4)
                    echo "self-test: cross arm NOT VERIFIED here ($label's preconditions are absent; recorded above)."
                    return 0 ;;
                5)
                    return 1 ;;
                *)
                    if [ "$ours" -ne 1 ]; then
                        echo "self-test FAILED: $label failed, but not on the injected missing source." >&2
                        echo "  The control cannot tell this arm's verdict from an unrelated breakage." >&2
                        return 1
                    fi
                    echo "self-test OK: a broken cross sub-project is caught (broke $label)."
                    return 0 ;;
            esac
        done < <(template_projects "$tmpl" 2>/dev/null)
    done
    echo "self-test FAILED: no template has a CMake cross sub-project for the cross arm's control." >&2
    return 1
}

mode="run"
case "${1:-}" in
    --list) mode="list"; shift ;;
    --self-test) mode="self-test"; shift ;;
esac

if [ "$mode" = "self-test" ]; then
    rc=0
    classifier_self_test || rc=1
    projects_self_test || rc=1
    self_test || rc=1
    cross_self_test || rc=1
    exit "$rc"
fi

wanted=("$@")
buildable=()
skipped=()
for tmpl in $(discover_templates); do
    if [ "${#wanted[@]}" -gt 0 ]; then
        # No pipeline here (issue 1077): a matcher that exits early can kill
        # the writer with SIGPIPE, and under `pipefail` a MATCH then reads as
        # a MISS — which would silently drop the template the caller asked for.
        want_hit=0
        for w in "${wanted[@]}"; do
            [ "$w" = "$tmpl" ] && { want_hit=1; break; }
        done
        [ "$want_hit" -eq 1 ] || continue
    fi
    rc=0
    projs="$(template_projects "$tmpl")" || rc=$?
    case "$rc" in
        0)
            while IFS= read -r proj; do
                buildable+=("$tmpl"$'\t'"$proj")
            done <<< "$projs"
            ;;
        1) skipped+=("$tmpl") ;;
        *)
            echo "check-template-copy-out: could not read $tmpl's images" >&2
            exit 1
            ;;
    esac
done

if [ "$mode" = "list" ]; then
    echo "projects (a tracked system.toml declares an [image.*]):"
    for entry in "${buildable[@]}"; do
        tmpl="${entry%%$'\t'*}"
        proj="${entry#*$'\t'}"
        printf '  %s  [%s]\n' "$(project_label "$tmpl" "$proj")" \
            "$(project_boards "$tmpl" "$proj" | paste -sd, -)"
    done
    echo "skipped (no [image.*] — nothing for \`nros build\` to build):"
    printf '  %s\n' "${skipped[@]}"
    exit 0
fi

if [ "${#buildable[@]}" -eq 0 ]; then
    echo "check-template-copy-out: no template declares an [image.*]." >&2
    echo "  That is not a pass — every copy-out template used to declare one," >&2
    echo "  so an empty set means discovery broke, not that the work is done." >&2
    exit 1
fi

echo "check-template-copy-out: building ${#buildable[@]} project(s) from a copy of the tracked file set"
fail=0
fail_build=0
missing_tool=0
strict_missing=0
verified=0
for entry in "${buildable[@]}"; do
    rc=0
    run_one "${entry%%$'\t'*}" "${entry#*$'\t'}" || rc=$?
    case "$rc" in
        0) verified=$((verified + 1)) ;;
        4) : ;;
        2) fail=1; missing_tool=1 ;;
        6) fail=1; strict_missing=1 ;;
        *) fail=1; fail_build=1 ;;
    esac
done
for tmpl in "${skipped[@]}"; do
    echo "  $tmpl: skipped — declares no [image.*]"
done

if [ "$fail" -ne 0 ]; then
    if [ "$missing_tool" -ne 0 ]; then
        # The summary must not assert a cause the run did not establish: a
        # missing CLI is not evidence about any template.
        echo "check-template-copy-out: FAILED — see above; at least one template" >&2
        echo "  could not be built because the tool was missing, which is not a" >&2
        echo "  finding about the template." >&2
    elif [ "$strict_missing" -ne 0 ] && [ "$fail_build" -eq 0 ]; then
        echo "check-template-copy-out: FAILED — a project's precondition is absent and" >&2
        echo "  NROS_CHECK_SKIP_STRICT=1 makes that a failure; no template was found broken." >&2
    else
        echo "check-template-copy-out: FAILED — a template a user copies does not build." >&2
    fi
    exit 1
fi
# The whole-tree run always has HOST projects, which need nothing but the CLI,
# so a run that built none of them has lost its discovery, not its SDKs. A run
# narrowed to named templates may legitimately land only on cross projects, and
# their skips are already in the ledger.
if [ "$verified" -eq 0 ] && [ "${#wanted[@]}" -eq 0 ]; then
    echo "check-template-copy-out: no project was BUILT — every one is NOT VERIFIED (see above)." >&2
    echo "  A lane that built nothing has verified nothing; that is not a pass." >&2
    exit 1
fi
echo "check-template-copy-out: OK — $verified project(s) built; any NOT VERIFIED one is listed above and in the lane's skip ledger"
