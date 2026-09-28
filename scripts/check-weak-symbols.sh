#!/usr/bin/env bash
# Issue 0050 / phase-247 W2 — fast source-level weak-symbol gate.
#
# Scans owned C/C++/asm sources for weak declarations and fails when a file
# outside the audited allowlist introduces one, or a listed file's weak-decl
# count drifts (a weak symbol added/removed without re-audit). Buildless +
# sub-second — fits the `just check` aggregate (cf. the other
# scripts/check-*.sh gates). The deeper per-platform *image* gate is
# scripts/check-weak-symbols-image.sh (needs prebuilt fixtures, runs under CI).
#
# Allowlist source of truth: scripts/weak-symbols-allowlist.txt (shared with
# nros-tests/tests/weak_symbol_audit.rs, which runs THIS script).
#
# REACH (issue 1543). This gate used to read `git ls-files 'packages/**'` and
# match exactly `__attribute__((weak))` and `.weak `. Three weak sites sat
# outside that for as long as it existed — `__attribute__((weak, used))` in the
# FreeRTOS C entry, and two files under `zephyr/`, a tree the pathspec never
# read. A weak symbol is a silent link-time override whichever tree it is in,
# and whichever of the compiler's spellings wrote it. So:
#   * files: every TRACKED C/C++/asm file in the repo (no pathspec), minus the
#     vendored/build/generated dirs below. Submodules are gitlinks, so their
#     sources are never listed — they are upstream's weak symbols, not ours.
#   * spellings: `__attribute__ ((... weak ...))` in any attribute list,
#     `__weak` (Zephyr/CMSIS macro), `#pragma weak`, `[[gnu::weak]]`, and the
#     asm `.weak <sym>` directive.
# The selftest at the bottom drives the SAME filter and counter the gate uses,
# with one case per spelling and one per previously-unread tree.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

# One ERE per spelling, joined. `[[:space:]]` rather than `\s` for POSIX grep.
#   __attribute__((weak)) / ((weak, used)) / (( used,weak )) / __attribute__ ((weak))
#   __weak                  (not `__weak_alias`, not `my__weak`)
#   #pragma weak sym
#   [[gnu::weak]] / [[gnu::used, gnu::weak]]
#   .weak sym               (asm, incl. inside an `asm("...")` string; a field
#                            access `x.weak = 1` is not followed by an identifier)
WEAK_ERE='__attribute__[[:space:]]*\(\([^)]*\bweak\b'
WEAK_ERE+='|(^|[^A-Za-z0-9_])__weak([^A-Za-z0-9_]|$)'
WEAK_ERE+='|#[[:space:]]*pragma[[:space:]]+weak\b'
WEAK_ERE+='|\[\[[^]]*\bgnu::weak\b'
WEAK_ERE+='|\.weak[[:space:]]+[A-Za-z_.$]'

# Which tracked paths are OWNED C/C++/asm. One predicate for the gate and the
# selftest.
owned_sources() {
    grep -E '\.(c|cc|cpp|cxx|h|hh|hpp|hxx|inc|S|s)$' \
        | grep -vE '(^|/)(target|build|generated|zenoh-pico|mbedtls|third-party)/'
}

# Weak declarations in one file, comments stripped first.
#
# The attribute is DISCUSSED in prose next to nearly every real use —
# "`__attribute__((weak))` so a C/C++ image can define this symbol strongly" —
# and counting those made the gate report drift that does not exist: phase-366
# added one such sentence to the threadx port and its count went 8 -> 9 with no
# new weak symbol. A gate that cries wolf on its own documentation gets
# bypassed (issue 0555 makes the same point).
#
# `cpp -fpreprocessed` removes comments without expanding anything, so
# `#include`s, macros and `#pragma`s are untouched and a `.S` file survives it.
# Falls back to the raw file if cpp is unavailable or chokes.
count_weak() {
    local stripped n rc=0
    stripped=$(cpp -fpreprocessed -dD -P "$1" 2>/dev/null) || stripped=$(cat "$1") || return 2
    # grep exits 1 for "no match" and 2 for an ERROR (a bad pattern) — only the
    # first is a zero count; the second must not read as "no weak symbols".
    n=$(grep -cE "$WEAK_ERE" <<<"$stripped") || rc=$?
    [ "$rc" -le 1 ] || return 2
    printf '%s\n' "${n:-0}"
}

# The gate proper: $1 = tree root, $2 = allowlist, stdin = candidate paths
# (relative to the root). Exit 0/1; diagnostics on stderr.
scan() {
    local root="$1" allowlist="$2"
    [ -f "$allowlist" ] || { echo "weak-source: missing $allowlist" >&2; return 1; }

    # Expected counts keyed by path (from the allowlist, comments/blank stripped).
    local -A expected=()
    local count path _rest
    while read -r count path _rest; do
        [ -z "${count:-}" ] && continue
        case "$count" in \#*) continue ;; esac
        expected["$path"]="$count"
    done < <(sed -E 's/#.*//' "$allowlist")

    # phase-386 W1 — every audited row must declare `body:<kind>`, the answer
    # to "if nobody overrides this, is the weak body CORRECT?". The
    # override-default/optional-hook classification answers a DIFFERENT
    # question (is a strong def guaranteed) and the two are independent, so a
    # row can be correctly classified there and still hide a stub that lies.
    #
    # Validated here rather than left as prose because an unchecked column
    # drifts. `silent-wrong` is deliberately NOT an accepted value — that
    # state is the bug this axis exists to surface, and a row needing it
    # should be fixed instead (phase-386 W2 removed the two that had it).
    local missing_body="" bad_body="" line rowpath body
    while IFS= read -r line; do
        case "$line" in \#*|"") continue;; esac
        rowpath=$(printf '%s' "$line" | awk '{print $2}')
        [ -n "$rowpath" ] || continue
        body=$(printf '%s' "$line" | sed -nE 's/.*body:([a-z-]+).*/\1/p')
        if [ -z "$body" ]; then
            missing_body="$missing_body  $rowpath"$'\n'
        else
            case "$body" in
                correct|reports-failure|self-enforcing) ;;
                *) bad_body="$bad_body  $rowpath -> body:$body"$'\n' ;;
            esac
        fi
    done < "$allowlist"

    if [ -n "$missing_body" ] || [ -n "$bad_body" ]; then
        echo "weak-source: allowlist rows with a missing/invalid \`body:\` axis:" >&2
        [ -n "$missing_body" ] && { echo "  MISSING:" >&2; printf '%s' "$missing_body" >&2; }
        [ -n "$bad_body" ] && { echo "  INVALID:" >&2; printf '%s' "$bad_body" >&2; }
        echo >&2
        echo "  Answer: if nobody overrides it, is the weak body CORRECT?" >&2
        echo "    body:correct         a valid runtime state; nothing is missing" >&2
        echo "    body:reports-failure says so in a form the CALLER understands" >&2
        echo "    body:self-enforcing  misuse faults immediately; do not 'fix' it" >&2
        echo >&2
        echo "  There is no body:silent-wrong. A row that would need it is the bug" >&2
        echo "  this axis exists to surface — fix the stub, do not label it." >&2
        return 1
    fi

    local -A actual=()
    local f n
    while IFS= read -r f; do
        n=$(count_weak "$root/$f") || {
            echo "weak-source: FAIL — could not scan $f (grep/cpp error)" >&2
            return 1
        }
        [ "${n:-0}" -gt 0 ] && actual["$f"]="$n"
    done < <(owned_sources)

    local fails=0
    # Unexpected (new unaudited site) + drifted counts.
    for f in "${!actual[@]}"; do
        if [ -z "${expected[$f]:-}" ]; then
            echo "  FAIL  $f: ${actual[$f]} weak decl(s) — NEW unaudited weak-symbol site." >&2
            echo "        Audit it (override-default vs optional-hook, strong-def source), then add to $allowlist." >&2
            fails=$((fails + 1))
        elif [ "${actual[$f]}" != "${expected[$f]}" ]; then
            echo "  FAIL  $f: weak-decl count ${actual[$f]}, allowlist expects ${expected[$f]} — re-audit + update $allowlist." >&2
            fails=$((fails + 1))
        fi
    done
    # Stale allowlist entries (file moved / weak removed).
    for f in "${!expected[@]}"; do
        if [ -z "${actual[$f]:-}" ]; then
            echo "  FAIL  $f: allowlisted but no weak decl found — drop it from $allowlist." >&2
            fails=$((fails + 1))
        fi
    done

    if [ "$fails" -gt 0 ]; then
        echo "weak-source: FAILED ($fails) — weak-symbol allowlist out of date (issue 0050)." >&2
        return 1
    fi
    echo "weak-source: ${#actual[@]} audited weak-symbol files OK."
    return 0
}

# --- selftest (issue 1543) ----------------------------------------------------
# Negative controls for every spelling and tree the gate used to miss, plus the
# comment-stripping and positive cases. A plain directory, not a git repo — the
# selftest feeds `scan` its own path list, so it needs no `git init` (and so
# cannot write into a caller's repository, issue 0986). The real run's
# enumeration is `git ls-files` with NO pathspec; the tree-reach floor below
# the selftest guards that half.
selftest() {
    local d rc=0 cases=0 out
    d="$(mktemp -d)"
    mkdir -p "$d/packages/a" "$d/packages/b" "$d/zephyr" "$d/packages/c/third-party"
    printf '%s\n' '__attribute__((weak)) int a_hook(void) { return 0; }' \
        '/* __attribute__((weak)) is discussed here and must not count */' \
        > "$d/packages/a/listed.c"
    printf '1  packages/a/listed.c  # body:correct optional-hook: selftest\n' > "$d/allow.txt"

    run_case() { # $1 label, $2 want (pass|fail), $3 extra allowlist line or ""
        local label="$1" want="$2" allow="$d/allow.case.txt" got
        cp "$d/allow.txt" "$allow"
        [ -n "$3" ] && printf '%s\n' "$3" >> "$allow"
        cases=$((cases + 1))
        if (cd "$d" && shopt -s globstar nullglob && for p in **/*; do
                if [ -f "$p" ] && [ "${p#allow}" = "$p" ]; then printf '%s\n' "$p"; fi; done \
                | scan "$d" "$allow") >/dev/null 2>&1; then got=pass; else got=fail; fi
        if [ "$got" != "$want" ]; then
            echo "weak-source: SELFTEST FAILED — $label: expected $want, got $got" >&2
            rc=1
        fi
    }

    run_case "positive control: audited tree" pass ""

    printf '__attribute__((weak, used)) void f(void) {}\n' > "$d/packages/b/new.c"
    run_case "__attribute__((weak, used)) in an unlisted file" fail ""
    run_case "... and passes once audited" pass \
        "1 packages/b/new.c # body:correct optional-hook: selftest"
    rm "$d/packages/b/new.c"

    printf 'int x __attribute__ ((used,weak));\n' > "$d/zephyr/stub.c"
    run_case "weak attribute in an unlisted file under zephyr/" fail ""
    rm "$d/zephyr/stub.c"

    local spelling
    for spelling in '__weak void g(void) {}' '#pragma weak g' '[[gnu::weak]] void g() {}'; do
        printf '%s\n' "$spelling" > "$d/packages/b/sp.cpp"
        run_case "spelling '$spelling' in an unlisted file" fail ""
    done
    rm "$d/packages/b/sp.cpp"

    printf '    .weak _start\n' > "$d/packages/b/start.S"
    run_case ".weak directive in an unlisted .S" fail ""
    rm "$d/packages/b/start.S"

    printf 'struct s { int weak; }; void h(struct s *p) { p->weak = 1; }\nint my__weak;\n' \
        > "$d/packages/b/not_weak.c"
    run_case "identifiers named weak / my__weak are not weak decls" pass ""
    rm "$d/packages/b/not_weak.c"

    printf '__attribute__((weak)) int v;\n' > "$d/packages/c/third-party/vendored.c"
    run_case "vendored third-party/ is out of scope" pass ""
    rm "$d/packages/c/third-party/vendored.c"

    printf '__attribute__((weak)) int b_hook(void) { return 0; }\n' >> "$d/packages/a/listed.c"
    run_case "count drift in a listed file" fail ""

    rm -rf "$d"
    [ "$rc" -eq 0 ] || return 1
    echo "weak-source: selftest OK ($cases cases)."
}

selftest || exit 1

cd "$repo_root"
tracked="$(git ls-files)" || { echo "weak-source: git ls-files failed" >&2; exit 1; }
owned="$(printf '%s\n' "$tracked" | owned_sources)"

# Tree-reach floor: the enumeration must see BOTH trees issue 1543 found it
# reading only one of. A pathspec creeping back would empty one of these.
for tree in packages/ zephyr/; do
    case $'\n'"$owned" in
        *$'\n'"$tree"*) continue ;;
    esac
    echo "weak-source: FAIL — no owned C/C++/asm under $tree was enumerated; the scan's reach has narrowed (issue 1543)." >&2
    exit 1
done

printf '%s\n' "$owned" | scan "$repo_root" "$script_dir/weak-symbols-allowlist.txt"
