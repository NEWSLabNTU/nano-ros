#!/usr/bin/env bash
# Issues 0400/0706 — the cmake `export -f` list must CLOSE over its call graph.
#
# `fixtures-build.sh` fans its cmake rows out to `make` workers. A make leaf is a
# fresh bash holding only what `export -f` gave it, so a function called by an
# exported function but missing from the list is an unbound command in the leaf —
# and only in the leaf, so it survives every local run of the same code.
#
# It has happened twice, each time as a helper ADDED to an already-exported
# function: 0400 added `nros_cmake_guard_build_dir` ("nros_cmake_guard_build_dir:
# command not found"), 0706 added `nros_cmake_toolchain_resolved_cc` and
# `nros_cmake_dir_cc` ("nros_cmake_toolchain_resolved_cc: command not found",
# which took out every NuttX C row of the tier-2 fixture build). Both were fixed
# by appending to the list — the reported site, not the class. This is the class:
# whoever adds the next helper does not have to remember the list exists.
#
# The CARGO half of the same list is already covered, by
# `build_root_derivation.sh`'s make-leaf scenario. This is its sibling; the two
# lists broke for the same reason and only one had a check.
#
# Run: bash scripts/check-cmake-export-closure.sh [--self-test]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DRIVER="$ROOT/scripts/build/fixtures-build.sh"
SOURCES=(
    "$ROOT/scripts/build/fixtures-build.sh"
    "$ROOT/scripts/build/cmake-incremental.sh"
    "$ROOT/scripts/build/cmake-cache-guard.sh"
)
ENTRY="nros_fixture_build_cmake"

# The `export -f` statement that starts with $ENTRY, continuation lines folded.
exported_list() {
    awk -v entry="$ENTRY" '
        $0 ~ ("export -f " entry) { c = 1 }
        c {
            cont = ($0 ~ /\\$/)
            sub(/export -f/, ""); sub(/\\$/, "")
            print
            if (!cont) exit
        }' "$1" | tr ' \t' '\n\n' | grep -v '^$' | sort -u
}

# Every `name() {` definition across the sourced files.
defined_funcs() {
    grep -hoE '^[a-zA-Z_][a-zA-Z_0-9]*\(\)' "$@" | tr -d '()' | sort -u
}

# Body of one function, from its definition line to the closing brace at depth 0.
body_of() {
    local func="$1"; shift
    awk -v f="$func" '
        $0 ~ ("^" f "\\(\\) \\{") { inside = 1; depth = 0 }
        inside {
            n = gsub(/\{/, "{"); m = gsub(/\}/, "}")
            depth += n - m
            print
            if (depth <= 0) exit
        }' "$@"
}

audit() {
    local driver="$1"; shift
    local sources=("$@")
    local exported defined missing=()
    # Space-separated: the membership tests below are `case " $x " in *" $c "*`,
    # and a newline-separated string never matches one.
    exported="$(exported_list "$driver" | tr '\n' ' ')"
    defined="$(defined_funcs "${sources[@]}")"

    # Transitive closure from the exported set. A helper reachable only through
    # another helper is exactly the 0706 shape.
    local queue=($exported) seen="" f body called
    while [ ${#queue[@]} -gt 0 ]; do
        f="${queue[0]}"; queue=("${queue[@]:1}")
        case " $seen " in *" $f "*) continue ;; esac
        seen="$seen $f"
        body="$(body_of "$f" "${sources[@]}" || true)"
        [ -n "$body" ] || continue
        # Calls to functions this project defines. The function's OWN name is
        # dropped rather than the first LINE: a one-line definition
        # (`f() { g; }`) is its whole body, and dropping the line drops the call.
        called="$(printf '%s\n' "$body" |
                  grep -ohE '\bnros_[a-zA-Z_0-9]+\b' | sort -u || true)"
        for c in $called; do
            [ "$c" = "$f" ] && continue
            printf '%s\n' "$defined" | grep -qx "$c" || continue
            case " $exported " in
                *" $c "*) ;;
                *) missing+=("$c (called by $f)") ;;
            esac
            queue+=("$c")
        done
    done

    if [ ${#missing[@]} -gt 0 ]; then
        printf '%s\n' "${missing[@]}" | sort -u
        return 1
    fi
    return 0
}

self_test() {
    local tmp ok=0
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN

    # A closed list passes.
    cat > "$tmp/src.sh" <<'EOF'
nros_helper() { echo hi; }
nros_fixture_build_cmake() { nros_helper; }
EOF
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_fixture_build_cmake nros_helper
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  ok    a closed list passes"
    else
        echo "  FAIL  a closed list passes"; ok=1
    fi

    # The 0400 shape: a called helper missing from the list.
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_fixture_build_cmake
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  FAIL  a directly-called helper missing from the list is reported"; ok=1
    else
        echo "  ok    a directly-called helper missing from the list is reported"
    fi

    # The 0706 shape: reachable only THROUGH another helper. A one-level check
    # passes this, which is why the closure is transitive.
    cat > "$tmp/src.sh" <<'EOF'
nros_deep() { echo deep; }
nros_helper() { nros_deep; }
nros_fixture_build_cmake() { nros_helper; }
EOF
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_fixture_build_cmake nros_helper
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  FAIL  a transitively-called helper is reported"; ok=1
    else
        echo "  ok    a transitively-called helper is reported"
    fi

    # A continuation line is part of the list, not a second statement.
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_fixture_build_cmake \
        nros_helper nros_deep
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  ok    a backslash continuation is read as one list"
    else
        echo "  FAIL  a backslash continuation is read as one list"; ok=1
    fi

    # A name that is merely MENTIONED (a comment, a message) is not a call to a
    # function nobody defines — no false positive from prose.
    cat > "$tmp/src.sh" <<'EOF'
nros_fixture_build_cmake() { echo "see nros_not_a_function for why"; }
EOF
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_fixture_build_cmake
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  ok    an undefined name in prose is not a missing export"
    else
        echo "  FAIL  an undefined name in prose is not a missing export"; ok=1
    fi

    return $ok
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit $?
fi

if missing="$(audit "$DRIVER" "${SOURCES[@]}")"; then
    echo "check-cmake-export-closure: OK (the cmake export -f list closes over its call graph)"
else
    echo "[FAIL] a cmake helper reaches a make leaf that cannot call it:" >&2
    # Quoted + indented: unquoted word-splitting broke "(called by …)" across
    # four lines, which is a diagnostic nobody can read.
    printf '%s\n' "$missing" | sed 's/^/  /' >&2
    echo >&2
    echo "  A make leaf is a fresh bash with only what \`export -f\` gave it, so this" >&2
    echo "  dies \"<name>: command not found\" in the WORKER and nowhere else" >&2
    echo "  (issues 0400, 0706)." >&2
    echo >&2
    echo "  Fix: add the name to the \`export -f $ENTRY …\` list in" >&2
    echo "  scripts/build/fixtures-build.sh." >&2
    exit 1
fi
