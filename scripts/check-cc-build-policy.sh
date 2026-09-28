#!/usr/bin/env bash
# issue 0478 — every `cc::Build` must carry the nano-ros cc policy.
#
# Two flag classes have now escaped through an unrouted `cc::Build::new()`:
#
#   issue 0383  the strict-declaration diagnostics
#   issue 0478  cc-rs handing gcc the clang-only `-mno-omit-leaf-frame-pointer`,
#               which gcc REJECTS — it killed every freertos fixture row
#
# Both were fixed by a shared helper in `nros-cc-flags`, and both then had call
# sites the helper never reached. This gate is the structural half: a file that
# constructs a `cc::Build` must also name the helper crate, so a new site cannot
# be added without deciding which policy it takes.
#
# `git grep` sees TRACKED files only, which is the repo rule (a filesystem walk
# over tracked paths measured 7m36s against 0.8s) and also means a brand-new
# build.rs is governed from the moment it is `git add`ed, not before. That is
# the right boundary for a pre-push gate, but it is a boundary.
#
# It checks PRESENCE per file, not per construction — a precise per-site check
# would need to parse Rust. That is deliberate under the issue-0196 rule: a
# narrow gate that looks healthy is worse than a coarse one that makes someone
# look. Widen it if a file ever mixes governed and ungoverned builds.
set -uo pipefail
cd "$(dirname "$0")/.."

# issue 0726 — `if ! grep -q nros_cc_flags::` reads a grep that never ran as
# "this build.rs is ungoverned", a specific claim about a file that is fine.
# `nros_grep_q` exits 2 on a tool failure instead of returning "no match".
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

# ONE matcher, used by the selftest AND the scan — a selftest that exercises a
# copy is not a control on the thing that runs (phase-472 W9).
#
# Prints the number of real `cc::Build::new()` constructions in "$1" when the
# file names no `nros_cc_flags::` call, and nothing when it is governed (or
# constructs nothing). Doc-comment examples are not construction sites
# (`threadx_sources.rs` is entirely `///` examples), so only code lines count.
ungoverned() {
    local f="$1" n
    n=$(grep -n "cc::Build::new()" "$f" | grep -vcE ':\s*(///|//!|\*|//)')
    [ "${n:-0}" -eq 0 ] && return 0
    if ! nros_grep_q "nros_cc_flags::" "$f"; then
        echo "$n"
    fi
}

# issue 1542 — the population is EVERY tracked `.rs`, not `packages/**`. The
# rule is about a `cc::Build`, and a copy-out example's build script drives the
# same cc-rs against the same arm-none-eabi-gcc as a board crate does; the old
# `packages/**/*.rs` root left `examples/mps2-an385-baremetal/c/talker/build.rs`
# outside the gate while the identical file under `packages/` failed it.
POPULATION=('*.rs')

# A subshell body, so the EXIT trap that removes the scratch dir stays local.
selftest() (
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    fails=0
    expect() { # <label> <want: flag|pass> <file>
        local got
        got="$(ungoverned "$3")"
        if [ "$2" = flag ] && [ -z "$got" ]; then
            echo "selftest: $1 — expected a finding, got none"; fails=1
        elif [ "$2" = pass ] && [ -n "$got" ]; then
            echo "selftest: $1 — expected no finding, got $got"; fails=1
        fi
    }
    printf '%s\n' 'fn main() {' '    let mut b = cc::Build::new();' '    b.compile("x");' '}' >"$tmp/bare.rs"
    expect "bare cc::Build" flag "$tmp/bare.rs"
    printf '%s\n' 'fn main() {' '    let mut b = cc::Build::new();' '    nros_cc_flags::strict_decls(&mut b);' '}' >"$tmp/governed.rs"
    expect "governed cc::Build" pass "$tmp/governed.rs"
    printf '%s\n' '/// let mut b = cc::Build::new();' '//! cc::Build::new()' 'fn main() {}' >"$tmp/doc.rs"
    expect "doc-comment only" pass "$tmp/doc.rs"
    # The reach half: a build.rs under examples/ must be IN the population.
    # `git ls-files` with the population pathspec over the tracked tree is the
    # same pathspec the scan hands `git grep`.
    # Captured first, not piped into a `-q` grep: an early exit SIGPIPEs
    # `git ls-files`, and under `pipefail` that reads as "no match".
    pop="$(git ls-files -- "${POPULATION[@]}")"
    nros_grep_q '^examples/.*/build\.rs$' <<<"$pop"
    if [ $? -ne 0 ]; then
        echo "selftest: population reaches no examples/**/build.rs (issue 1542)"; fails=1
    fi
    [ "$fails" -eq 0 ] || { echo "selftest FAILED"; return 1; }
    echo "check-cc-build-policy selftest: OK"
)

if [ "${1:-}" = "--selftest" ]; then
    selftest
    exit $?
fi

# Always, not only behind --selftest: a negative control nobody runs decays into
# a comment.
selftest >/dev/null || {
    echo "check-cc-build-policy: its own selftest FAILED — the gate is not trustworthy" >&2
    exit 1
}

fail=0
while IFS= read -r f; do
    n="$(ungoverned "$f")"
    if [ -n "$n" ]; then
        echo "  $f — $n cc::Build::new() and no nros_cc_flags:: call"
        fail=1
    fi
done < <(git grep -l "cc::Build::new()" -- "${POPULATION[@]}" 2>/dev/null)

if [ "$fail" -ne 0 ]; then
    cat >&2 <<'EOF'

Every cc::Build must carry the nano-ros cc policy (issues 0383, 0478).

  C compiles:    nros_cc_flags::strict_decls(&mut build);
                 (it applies the frame-pointer policy too)
  C++ compiles:  nros_cc_flags::gcc_safe_frame_pointer(&mut build);
                 (strict_decls is C-only — do NOT call it on a C++ build)

EOF
    exit 1
fi
echo "check-cc-build-policy: OK (every file constructing a cc::Build names the policy helper)"
