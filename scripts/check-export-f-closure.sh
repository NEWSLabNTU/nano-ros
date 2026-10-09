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
    # Command position only (`^ *export -f`): a COMMENT that quotes the
    # statement (`# \`build_one\` is \`export -f\`'d into …`) is not a list, and
    # reading it as one would put prose words into the exported set and mask a
    # real gap.
    awk '
        /^[[:space:]]*export -f/ { c = 1 }
        c {
            cont = ($0 ~ /\\$/)
            sub(/.*export -f/, ""); sub(/\\$/, "")
            print
            if (!cont) c = 0
        }' "$@" | tr ' \t' '\n\n' |
        # Identifiers only: `export -f a b 2>/dev/null || true` (checkout-paths.sh)
        # puts a redirection and an operator on the same line as the names.
        grep -E '^[A-Za-z_][A-Za-z_0-9]*$' | sort -u
}

# Every `name() {` definition across the files — INDENTED ones included.
#
# Issue 1620: this read column-0 definitions only, and the two functions
# `fixtures-build.sh` actually ships to its make leaves (`nros_fixture_build_one`,
# `nros_fixture_check_stack_floor`) are defined inside an `if` block, four spaces
# in. So the walk started from them found no body, checked nothing, and printed
# OK while every esp32 row died in its leaf on an unexported callee
# (`nros_fixture_row_artifact_dir`). The issue-0196 shape: a gate whose reach is
# narrower than the rule it enforces, on exactly the definitions that matter.
defined_funcs() {
    grep -hoE '^[[:space:]]*[a-zA-Z_][a-zA-Z_0-9]*\(\)' "$@" | tr -d '() \t' | sort -u
}

# Body of one function, from its definition line to the closing brace at depth 0.
body_of() {
    local func="$1"; shift
    awk -v f="$func" '
        !inside && $0 ~ ("^[[:space:]]*" f "\\(\\) \\{") { inside = 1; depth = 0 }
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
    grep -lE "^[[:space:]]*${func}\(\)" "$@" 2>/dev/null | head -1 | sed "s|^$ROOT/||"
}

# `audit [--local "name …"] <files…>`. `--local` names the site's OWN helpers
# (`build_one`, `check_one`, `run_talker`): they are not `nros_*`, so the walk
# below would not see a call to one without being told the names. Issue 1656 —
# that is why the gate read `scripts/build/*.sh` only: widening the file set
# without this would have reported OK over closures it could not see.
audit() {
    local locals=""
    if [ "${1:-}" = "--local" ]; then locals="$2"; shift 2; fi
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
        # Whole-line comments are dropped first: a DEFINED function named in a
        # comment is not a call (fixtures-build.sh's leaf body cites the parent's
        # `nros_presync_row_dirs` while explaining why it no longer calls it).
        # Only whole lines — a trailing `#` is too often `$#` / `${x#…}`.
        local pat='\bnros_[a-zA-Z_0-9]+\b'
        [ -n "$locals" ] && pat="$pat|\\b($(echo $locals | tr ' ' '|'))\\b"
        called="$(printf '%s\n' "$body" | grep -vE '^[[:space:]]*#' |
                  grep -ohE "$pat" | sort -u || true)"
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

    # Issue 1620 — a definition INDENTED inside a block (fixtures-build.sh
    # defines its leaf functions inside an `if`) is walked like any other. A
    # column-0-only reader found no body for it and passed vacuously.
    cat > "$tmp/src.sh" <<'EOF'
nros_helper() { echo hi; }
if true; then
    nros_entry() {
        nros_helper
    }
fi
EOF
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_entry
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  FAIL  an indented definition's callees are walked"; ok=1
    else
        echo "  ok    an indented definition's callees are walked"
    fi

    # A DEFINED function cited in a whole-line comment is not a call.
    cat > "$tmp/src.sh" <<'EOF'
nros_parent_only() { echo parent; }
nros_entry() {
    # this used to call nros_parent_only; the parent does it now
    echo leaf
}
EOF
    cat > "$tmp/driver.sh" <<'EOF'
    export -f nros_entry
EOF
    if audit "$tmp/driver.sh" "$tmp/src.sh" >/dev/null; then
        echo "  ok    a defined function named in a comment is not a missing export"
    else
        echo "  FAIL  a defined function named in a comment is not a missing export"; ok=1
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

    # Issue 1656 — a site's OWN helper (not `nros_*`) is followed when the site
    # names its locals: `export -f run_talker` whose body calls an unexported
    # local `wait_router` dies in the subshell exactly like the 0400 shape.
    cat > "$tmp/site.sh" <<'EOF'
wait_router() { sleep 1; }
run_talker() {
    wait_router
}
    export -f run_talker
EOF
    if audit --local "wait_router run_talker" "$tmp/site.sh" >/dev/null; then
        echo "  FAIL  a site-local helper missing from the list is reported"; ok=1
    else
        echo "  ok    a site-local helper missing from the list is reported"
    fi
    # ...and a COMMENT quoting the statement is not a list (it would have put
    # `wait_router` into the exported set and masked the gap above).
    cat > "$tmp/site.sh" <<'EOF'
wait_router() { sleep 1; }
run_talker() {
    wait_router
}
# `wait_router` is `export -f`'d below, see …
    export -f run_talker
EOF
    if audit --local "wait_router run_talker" "$tmp/site.sh" >/dev/null; then
        echo "  FAIL  an export -f quoted in a comment is not a list"; ok=1
    else
        echo "  ok    an export -f quoted in a comment is not a list"
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

# The shared libraries every site may source: their `nros_*` definitions and
# their own `export -f` lists (which run when sourced).
LIBS=("$ROOT"/scripts/build/*.sh "$ROOT"/scripts/lib/*.sh)

# Every file that RUNS an `export -f` — issue 1656: this read `scripts/build/*.sh`
# only, leaving the lists in the root `justfile`, `just/native.just` and
# `scripts/debug/debug-keyexpr.sh` closed by hand. Derived from the tree, never
# listed. This gate's own file is excluded: its self-test fixtures are heredocs.
mapfile -t SITES < <(cd "$ROOT" && git ls-files -- 'scripts/*.sh' 'scripts/**/*.sh' \
        justfile 'just/*.just' 'just/**/*.just' |
    grep -v '^scripts/check-export-f-closure\.sh$' |
    xargs grep -lE '^[[:space:]]*export -f' | sed "s|^|$ROOT/|")

failed=0 report="" names=0
lib_defined=" $(defined_funcs "${LIBS[@]}" | tr '\n' ' ') "
for site in "${SITES[@]}"; do
    # The site's OWN helpers — defined here and not in a shared library.
    locals=""
    for f in $(defined_funcs "$site"); do
        case "$lib_defined" in *" $f "*) ;; *) locals="$locals $f" ;; esac
    done
    if out="$(audit --local "$locals" "$site" "${LIBS[@]}")"; then
        :
    else
        failed=1
        report="$report$(printf '%s\n' "$out" | sed "s|^|  ${site#$ROOT/}: |")"$'\n'
    fi
    names=$((names + $(exported_list "$site" | wc -l)))
done

if [ "$failed" -eq 0 ]; then
    echo "check-export-f-closure: OK ($names exported name(s) across ${#SITES[@]} site(s): $(printf '%s ' "${SITES[@]#$ROOT/}"))"
else
    echo "[FAIL] an exported helper reaches a subshell that cannot call it:" >&2
    printf '%s' "$report" >&2
    echo >&2
    echo "  A make leaf (or a GNU parallel / jobserver subshell) is a fresh bash with" >&2
    echo "  only what \`export -f\` gave it, so this dies \"<name>: command not found\"" >&2
    echo "  in the WORKER and nowhere else (issues 0400, 0706, 0712)." >&2
    echo >&2
    echo "  Fix: add the name to that site's \`export -f\` list." >&2
    exit 1
fi
