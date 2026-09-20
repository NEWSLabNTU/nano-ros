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
# ## Which templates
#
# DISCOVERED, never listed. A template is buildable iff some tracked
# `system.toml` under it declares an `[image.<id>]` — that is exactly the
# question `nros build` asks (RFC-0098 D3), so this cannot drift from it. The
# issue named four templates; six declare an image today and the list in the
# issue was already stale when it was written. Templates that declare none are
# REPORTED as skipped with that reason, never silently dropped.
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

templates_dir="examples/templates"

# Every tracked template DIRECTORY, one per line. `NF > 3` is what makes it a
# directory rather than a file: `examples/templates/README.md` has three fields
# and is not a template, and reporting it as one made the skip list read as if
# the tree had two more templates than it has.
discover_templates() {
    git ls-files "$templates_dir" \
        | awk -F/ 'NF > 3 { print $3 }' \
        | sort -u
}

# Print the tracked system.toml under a template that declares an image, or
# nothing. `[image.` is the shape `nros build` reads; asking the same question
# keeps this from drifting away from the builder.
image_declaring_manifest() {
    local tmpl="$1" f
    for f in $(git ls-files "$templates_dir/$tmpl" | grep '/system\.toml$'); do
        nros_grep_q '^\[image\.' "$f"
        case $? in
            0) printf '%s' "$f"; return 0 ;;
            1) : ;;
        esac
    done
    return 1
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

# Build one copied-out template. Echoes the log path; returns 0 on success.
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

run_one() {
    local tmpl="$1" work dest log arts n
    work="$(mktemp -d "${TMPDIR:-/tmp}/nros-template-copy-out.XXXXXX")" || return 3
    dest="$work/copy"
    log="$work/build.log"
    if ! copy_out "$tmpl" "$dest"; then
        echo "  $tmpl: FAIL — could not copy the tracked file set out" >&2
        rm -rf "$work"
        return 1
    fi
    local ws="$dest/$templates_dir/$tmpl" brc=0
    build_copy "$ws" "$log" || brc=$?
    if [ "$brc" -eq 3 ]; then
        # NOT a template failure, and saying so matters: in a pristine worktree
        # this arm is the whole verdict, and the first spelling reported
        # "the copy does not build" over a `sed: can't read .../build.log`,
        # which blames the template for a missing tool.
        echo "  $tmpl: FAIL — no \`nros\` CLI to build with" >&2
        echo "      Looked at \$NROS_CLI, packages/cli/target/release/nros," >&2
        echo "      then PATH. Build it: just setup-cli" >&2
        return 2
    fi
    if [ "$brc" -ne 0 ]; then
        echo "  $tmpl: FAIL — the copy does not build" >&2
        if [ -s "$log" ]; then
            sed -n '1,12p' "$log" | sed 's/^/      /' >&2
            echo "      full log: $log" >&2
        fi
        return 1
    fi
    arts="$(built_artifacts "$ws")"
    n="$(printf '%s' "$arts" | grep -c . )"
    if [ "$n" -eq 0 ]; then
        echo "  $tmpl: FAIL — build exited 0 but produced no executable" >&2
        echo "      (rc=0 with an empty artifact set is the vacuous pass this" >&2
        echo "       predicate exists to catch; full log: $log)" >&2
        return 1
    fi
    echo "  $tmpl: OK — $n artifact(s), e.g. ${arts%%$'\n'*}" | sed "s|$ws/||"
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

self_test() {
    # Negative control. Break a copy the way the real defect did — a <depend>
    # that resolves to nothing — and require a FAIL. Uses the first buildable
    # template with a package.xml, so it cannot be outlived by a rename.
    local tmpl work dest ws pkg rc
    for tmpl in $(discover_templates); do
        image_declaring_manifest "$tmpl" >/dev/null || continue
        [ -n "$(git ls-files "$templates_dir/$tmpl" | grep '/package\.xml$' | head -1)" ] || continue
        break
    done
    [ -n "${tmpl:-}" ] || { echo "self-test: no buildable template to break" >&2; return 2; }

    work="$(mktemp -d "${TMPDIR:-/tmp}/nros-template-selftest.XXXXXX")" || return 2
    dest="$work/copy"
    copy_out "$tmpl" "$dest" || { echo "self-test: copy failed" >&2; return 2; }
    ws="$dest/$templates_dir/$tmpl"
    pkg="$(find "$ws" -name package.xml | head -1)"
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

mode="run"
case "${1:-}" in
    --list) mode="list"; shift ;;
    --self-test) mode="self-test"; shift ;;
esac

if [ "$mode" = "self-test" ]; then
    rc=0
    classifier_self_test || rc=1
    self_test || rc=1
    exit "$rc"
fi

wanted=("$@")
buildable=()
skipped=()
for tmpl in $(discover_templates); do
    if [ "${#wanted[@]}" -gt 0 ]; then
        printf '%s\n' "${wanted[@]}" | nros_grep_q -Fx "$tmpl" || continue
    fi
    if manifest="$(image_declaring_manifest "$tmpl")"; then
        buildable+=("$tmpl")
    else
        skipped+=("$tmpl")
    fi
done

if [ "$mode" = "list" ]; then
    echo "buildable (a tracked system.toml declares an [image.*]):"
    printf '  %s\n' "${buildable[@]}"
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

echo "check-template-copy-out: building ${#buildable[@]} template(s) from a copy of the tracked file set"
fail=0
missing_tool=0
for tmpl in "${buildable[@]}"; do
    rc=0
    run_one "$tmpl" || rc=$?
    [ "$rc" -eq 0 ] || fail=1
    [ "$rc" -eq 2 ] && missing_tool=1
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
    else
        echo "check-template-copy-out: FAILED — a template a user copies does not build." >&2
    fi
    exit 1
fi
echo "check-template-copy-out: OK"
