#!/usr/bin/env bash
# Issues 0400/0706/0712 — every `export -f` list must CLOSE over its call graph.
#
# The build shell files fan work out to `make` workers. A make leaf is a fresh
# bash holding only what `export -f` gave it, so a function called by an exported
# function but missing from the lists is an unbound command in the LEAF — and
# only in the leaf, so it survives every local run of the same code and surfaces
# after a long build, naming the callee but not the cause.
#
# Three occurrences, all the same shape — a helper ADDED to an already-exported
# function, with the list in a different file and nothing connecting them:
#
#   issue 0400      nros_cmake_guard_build_dir
#   phase-340 B2    nros_fixture_platform_is_shared
#   issue 0706      nros_cmake_toolchain_resolved_cc, nros_cmake_dir_cc
#
# The third took out every NuttX C row of the tier-2 fixture build.
#
# # Why this replaces `check-cmake-export-closure.sh`
#
# That gate (issue 0717) checked ONE list: the closure from
# `nros_fixture_build_cmake`. Its own justification was that "the CARGO half of
# the same list is already covered by `build_root_derivation.sh`'s make-leaf
# scenario" — true, and far narrower than it sounds. That scenario EXECUTES
# `nros_fixture_target_dir_flag` in a fresh bash with the list applied, so it
# proves only the path those arguments take; a helper on a branch not taken is
# invisible to it. Between the two, 2 of the tree's `export -f` statements had
# any coverage and the rest had none — the issue-0196 shape, a gate narrower
# than the rule it enforces.
#
# So the unit is no longer an entry point. Every `export -f` in the build shell
# files contributes to one exported SET, and every function in that set must be
# able to call what it calls.
#
# Run: bash scripts/check-export-f-closure.sh [--self-test]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Every `export -f` list in the file(s), continuation lines folded. A naive
# line-based reader sees only the first row of a `\`-wrapped list and passes
# vacuously — which is how a list can grow without the check noticing.
exported_list() {
    awk '
        /export -f/ { c = 1 }
        c {
            cont = ($0 ~ /\\$/)
            sub(/.*export -f/, ""); sub(/\\$/, "")
            print
            if (!cont) c = 0
        }' "$@" | tr ' \t' '\n\n' | grep -v '^$' | sort -u
}

# Every `name() {` definition across the files.
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

# Which file defines a name — so the diagnostic says where to look, not just what.
defined_in() {
    local func="$1"; shift
    grep -lE "^${func}\(\)" "$@" 2>/dev/null | head -1 | sed "s|^$ROOT/||"
}

audit() {
    local sources=("$@")
    local exported defined missing=()
    # Space-separated: the membership tests below are `case " $x " in *" $c "*`,
    # and a newline-separated string never matches one.
    exported="$(exported_list "${sources[@]}" | tr '\n' ' ')"
    # Space-separated for the same reason, and additionally so membership is a
    # bash `case` rather than a forked `grep` per candidate: this runs inside the
    # BFS below, and issue 0726 is about what a grep that fails to start does to
    # a checker's verdict. Not forking is a better answer than handling it.
    defined="$(defined_funcs "${sources[@]}" | tr '\n' ' ')"

    # Transitive closure from the exported set. A helper reachable only THROUGH
    # another helper is exactly the 0706 shape, and a one-level check passes it.
    local queue=($exported) seen="" f body called c
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
            case " $defined " in *" $c "*) ;; *) continue ;; esac
            case " $exported " in
                *" $c "*) ;;
                *) missing+=("$c (called by $f, defined in $(defined_in "$c" "${sources[@]}"))") ;;
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
nros_entry() { nros_helper; }
EOF
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_entry nros_helper
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  ok    a closed list passes"
    else
        echo "  FAIL  a closed list passes"; ok=1
    fi

    # The 0400 shape: a called helper missing from the list.
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_entry
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  FAIL  a directly-called helper missing from the list is reported"; ok=1
    else
        echo "  ok    a directly-called helper missing from the list is reported"
    fi

    # The 0706 shape: reachable only THROUGH another helper.
    cat > "$tmp/src.sh" <<'EOF'
nros_deep() { echo deep; }
nros_helper() { nros_deep; }
nros_entry() { nros_helper; }
EOF
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_entry nros_helper
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  FAIL  a transitively-called helper is reported"; ok=1
    else
        echo "  ok    a transitively-called helper is reported"
    fi

    # A continuation line is part of the list, not a second statement.
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_entry \
        nros_helper nros_deep
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  ok    a backslash continuation is read as one list"
    else
        echo "  FAIL  a backslash continuation is read as one list"; ok=1
    fi

    # SEVERAL lists in one file are ONE exported set — the generalisation this
    # gate exists for. `fixtures-build.sh` alone carries six `export -f`
    # statements; reading only the first is the vacuous pass above, and treating
    # each as its own closure would report every cross-list call as missing.
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_entry
    export -f nros_helper \
        nros_deep
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  ok    several export -f statements form one exported set"
    else
        echo "  FAIL  several export -f statements form one exported set"; ok=1
    fi

    # A helper defined in a SIBLING file still has to be exported. This is the
    # 0400/0706 geometry: the list lives in the driver, the helper in the file
    # the driver sources, and nothing links them.
    cat > "$tmp/src.sh" <<'EOF'
nros_entry() { nros_sibling; }
EOF
    cat > "$tmp/other.sh" <<'EOF'
nros_sibling() { echo from a sibling file; }
EOF
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_entry
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" "$tmp/other.sh" >/dev/null; then
        echo "  FAIL  a helper defined in a sibling file must be exported"; ok=1
    else
        echo "  ok    a helper defined in a sibling file must be exported"
    fi

    # A name that is merely MENTIONED (a comment, a message) is not a call to a
    # function nobody defines — no false positive from prose.
    cat > "$tmp/src.sh" <<'EOF'
nros_entry() { echo "see nros_not_a_function for why"; }
EOF
    rm -f "$tmp/other.sh"
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_entry
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  ok    an undefined name in prose is not a missing export"
    else
        echo "  FAIL  an undefined name in prose is not a missing export"; ok=1
    fi

    # A checker that stops checking passes silently, which is the failure shape
    # this issue is about — so assert the real tree has lists to read at all.
    local n
    n="$(exported_list "$ROOT"/scripts/build/*.sh | wc -l)"
    if [ "$n" -gt 0 ]; then
        echo "  ok    the real build files yield an exported set ($n name(s))"
    else
        echo "  FAIL  read NO exported names from scripts/build/*.sh"; ok=1
    fi

    return $ok
}

# The negative control runs on EVERY invocation, not only behind the flag.
#
# `check-gate-selftests` states the reason: *a negative control nobody runs
# decays into a comment.* Behind `--self-test` this was run once, by its author,
# on the day it was written. It costs 0.12 s.
#
# The flag is kept for running the control ALONE while working on it — it now
# exits straight after, rather than being the only way to reach it.
self_test || exit 1
if [ "${1:-}" = "--self-test" ]; then
    exit 0
fi

SOURCES=("$ROOT"/scripts/build/*.sh)
LISTS="$(grep -lE 'export -f' "${SOURCES[@]}" | sed "s|^$ROOT/||" | tr '\n' ' ')"

if missing="$(audit "${SOURCES[@]}")"; then
    echo "check-export-f-closure: OK ($(exported_list "${SOURCES[@]}" | wc -l) exported name(s) across ${LISTS% })"
else
    echo "[FAIL] an exported helper reaches a make leaf that cannot call it:" >&2
    # Quoted + indented: unquoted word-splitting broke "(called by …)" across
    # several lines, which is a diagnostic nobody can read.
    printf '%s\n' "$missing" | sed 's/^/  /' >&2
    echo >&2
    echo "  A make leaf is a fresh bash with only what \`export -f\` gave it, so this" >&2
    echo "  dies \"<name>: command not found\" in the WORKER and nowhere else" >&2
    echo "  (issues 0400, 0706, 0712)." >&2
    echo >&2
    echo "  Fix: add the name to an \`export -f\` list in one of:" >&2
    echo "    ${LISTS% }" >&2
    exit 1
fi
