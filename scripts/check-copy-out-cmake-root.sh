#!/usr/bin/env bash
# A copy-out gate configures against THIS checkout, never an inherited root —
# issue 1925.
#
# `check-template-copy-out` and `check-scaffold-builds` copy a project out of
# the checkout and configure it with CMake. Outside the checkout there is no
# walk-up, so `find_package(nano_ros)` reads whatever `nano_ros_ROOT` the
# caller's environment holds: nothing in a clean shell (configure fails), the
# PARENT checkout in a linked worktree (the gate measures the wrong tree —
# issue 1280's class). Both were measured; `just` hid them only because
# `just/sdk-env.just` exports this checkout's root to every recipe.
#
# Two checks, both cheap (no compilation):
#
#   1. BEHAVIOUR — `nros_copy_out_cmake_configure` (scripts/build/copy-out-cmake.sh)
#      is run against a stub `cmake` with `nano_ros_ROOT` and `NROS_REPO_DIR`
#      inherited from a foreign path. The stub records its argv and
#      environment; the argv must name `-Dnano_ros_ROOT=<this checkout>` and
#      the environment must hold neither inherited value.
#
#   2. SOURCE — no gate script (`scripts/check-*.sh`, `scripts/check/*.sh`)
#      runs a bare `cmake` CONFIGURE (`-S` / `-B` on the line): it goes through
#      the helper, or carries `# copy-out-root-ok: <reason>` for a configure of
#      something that is not a nano-ros consumer. Reach, stated: the rule is
#      about gate scripts, which are where copy-outs live; `tests/*.sh` name
#      their root explicitly (`export nano_ros_ROOT="$PROJECT_ROOT"`) and
#      in-tree builds walk up (`cmake-incremental.sh`), so neither is scanned.
#
# `--self-test` feeds the source rule a bare configure (must be flagged) and a
# helper call and a `--build` (must not be), so check 2 is seen to fail.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root" || exit 2

# shellcheck source=scripts/lib/grep-q.sh
. "$repo_root/scripts/lib/grep-q.sh"

# A line that CONFIGURES with a bare `cmake`: the word `cmake` in command
# position, a `-S`/`-B` on the line, not a `--build`/`--install`, not a comment,
# not the helper, not exempted.
bare_configures() {
    grep -HnE '(^|[[:space:];(|&!])cmake[[:space:]].*[[:space:]](-S|-B)([[:space:]"]|$)' "$@" \
        | grep -vE '^[^:]*:[0-9]+:[[:space:]]*#' \
        | grep -vE -- '--build|--install|nros_copy_out_cmake_configure|copy-out-root-ok:'
}

source_self_test() {
    local tmp out rc
    tmp="$(mktemp -d)" || return 2
    # `@TOOL@` stands for the tool name and is substituted below, so these
    # fixture lines are not themselves bare configures in THIS file, which the
    # source rule scans.
    cat > "$tmp/bad.in" <<'EOF'
@TOOL@ -S "$dir" -B "$dir/build" -DCMAKE_BUILD_TYPE=Release > "$log" 2>&1 || rc=$?
        @TOOL@ "${gen[@]}" -S "$proj" -B "$proj/build" -DCMAKE_TOOLCHAIN_FILE="$tc"
EOF
    cat > "$tmp/good.in" <<'EOF'
nros_copy_out_cmake_configure -S "$dir" -B "$dir/build"
@TOOL@ --build "$dir/build"
# @TOOL@ -S x -B y   (prose)
@TOOL@ -S "$x" -B "$y"   # copy-out-root-ok: a probe that never finds nano_ros
EOF
    sed 's/@TOOL@/cmake/g' "$tmp/bad.in" > "$tmp/bad.sh" || return 2
    sed 's/@TOOL@/cmake/g' "$tmp/good.in" > "$tmp/good.sh" || return 2
    out="$(bare_configures "$tmp/bad.sh")"
    rc=0
    [ "$(printf '%s\n' "$out" | sed '/^$/d' | wc -l)" -eq 2 ] || rc=1
    out="$(bare_configures "$tmp/good.sh")"
    [ -z "$out" ] || rc=1
    rm -rf "$tmp"
    if [ "$rc" -ne 0 ]; then
        echo "check-copy-out-cmake-root self-test: FAIL — the source rule misclassifies" >&2
        return 1
    fi
    echo "check-copy-out-cmake-root self-test: OK (2 bare configures flagged, helper/--build/exempt/comment passed)"
}

behaviour_check() {
    local tmp foreign rc=0
    tmp="$(mktemp -d)" || return 2
    foreign="$tmp/some-other-checkout"
    mkdir -p "$tmp/bin" "$foreign"
    cat > "$tmp/bin/cmake" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$@" > "$STUB_OUT.argv"
env > "$STUB_OUT.env"
EOF
    chmod +x "$tmp/bin/cmake"
    (
        export PATH="$tmp/bin:$PATH" STUB_OUT="$tmp/rec"
        export nano_ros_ROOT="$foreign" NROS_REPO_DIR="$foreign"
        # shellcheck source=scripts/build/copy-out-cmake.sh
        . "$repo_root/scripts/build/copy-out-cmake.sh"
        nros_copy_out_cmake_configure -S "$tmp/src" -B "$tmp/src/build"
    ) || rc=$?
    if [ "$rc" -ne 0 ] || [ ! -f "$tmp/rec.argv" ]; then
        echo "check-copy-out-cmake-root: FAIL — the helper did not run cmake (rc=$rc)" >&2
        rm -rf "$tmp"; return 1
    fi
    local want="-Dnano_ros_ROOT=$repo_root" fail=0
    local grc=0
    nros_grep_q -xF -- "$want" "$tmp/rec.argv" || grc=$?
    [ "$grc" -le 1 ] || { echo "check-copy-out-cmake-root: grep failed on $tmp/rec.argv" >&2; rm -rf "$tmp"; return 2; }
    if [ "$grc" -eq 1 ]; then
        echo "check-copy-out-cmake-root: FAIL — the configure does not name this checkout:" >&2
        echo "  wanted '$want' among cmake's arguments, got:" >&2
        sed 's/^/    /' "$tmp/rec.argv" >&2
        fail=1
    fi
    grc=0
    nros_grep_q -E '^(nano_ros_ROOT|NROS_REPO_DIR)=' "$tmp/rec.env" || grc=$?
    [ "$grc" -le 1 ] || { echo "check-copy-out-cmake-root: grep failed on $tmp/rec.env" >&2; rm -rf "$tmp"; return 2; }
    if [ "$grc" -eq 0 ]; then
        echo "check-copy-out-cmake-root: FAIL — an inherited root reached the configure's environment:" >&2
        grep -E '^(nano_ros_ROOT|NROS_REPO_DIR)=' "$tmp/rec.env" | sed 's/^/    /' >&2
        fail=1
    fi
    rm -rf "$tmp"
    [ "$fail" -eq 0 ] || return 1
    echo "check-copy-out-cmake-root: helper names $repo_root under an inherited foreign root"
}

case "${1:-}" in
    --self-test) source_self_test; exit $? ;;
    "") ;;
    *) echo "usage: $0 [--self-test]" >&2; exit 2 ;;
esac

source_self_test || exit 1
behaviour_check || exit 1

files=()
while IFS= read -r f; do files+=("$f"); done < <(git ls-files 'scripts/check-*.sh' 'scripts/check/*.sh')
if [ "${#files[@]}" -eq 0 ]; then
    echo "check-copy-out-cmake-root: FAIL — scanned NO gate scripts (an empty scan passes vacuously)" >&2
    exit 1
fi
hits="$(bare_configures "${files[@]}")"
if [ -n "$hits" ]; then
    echo "check-copy-out-cmake-root: FAIL — a gate configures CMake without naming this checkout (issue 1925):" >&2
    printf '%s\n' "$hits" | sed 's/^/  /' >&2
    echo "  Configure through nros_copy_out_cmake_configure (scripts/build/copy-out-cmake.sh)," >&2
    echo "  or mark a non-nano-ros configure '# copy-out-root-ok: <reason>'." >&2
    exit 1
fi
echo "check-copy-out-cmake-root: OK — ${#files[@]} gate script(s), no bare cmake configure"
