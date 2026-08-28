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

fail=0
while IFS= read -r f; do
    # Doc-comment examples are not construction sites (`threadx_sources.rs` is
    # entirely `///` examples). Count only lines that are real code.
    n=$(grep -n "cc::Build::new()" "$f" | grep -vcE ':\s*(///|//!|\*|//)' )
    [ "${n:-0}" -eq 0 ] && continue
    if ! nros_grep_q "nros_cc_flags::" "$f"; then
        echo "  $f — $n cc::Build::new() and no nros_cc_flags:: call"
        fail=1
    fi
done < <(git grep -l "cc::Build::new()" -- 'packages/**/*.rs' 2>/dev/null)

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
