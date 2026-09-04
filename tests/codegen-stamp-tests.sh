#!/bin/bash
# tests/codegen-stamp-tests.sh -- issue 1018
#
# Gates the DECISION RULE of the `generated/` regeneration stamp:
#
#   scripts/build/codegen-stamp.sh   nros_codegen_stamp_compute
#
# THE CONTRACT, two halves that must hold together:
#
#   1. the stamp MOVES when the tool would EMIT different bytes;
#   2. the stamp DOES NOT MOVE when only the tool's BINARY moved.
#
# WHY BOTH. Half (1) alone is satisfied by hashing the `nros` binary, and that
# is the fix phase-424 forbids: measured on this host 2026-09-05, 168 distinct
# `nros` binaries produced 11 distinct codegen fingerprints, so a binary-keyed
# stamp wipes and re-syncs every leaf's `generated/` on 157 rebuilds that emit
# identical code. Half (2) alone is satisfied by a stamp that never moves, which
# is issue 1018 as filed: the stamp watched `nros-core/src/action.rs` and
# NOTHING about the emitters, so an edit to `rosidl-codegen` left every cached
# `generated/` tree in place. In `just/zephyr-ci.just` -- the one caller whose
# `nros sync` is conditional -- that means the leaf compiles message crates the
# previous CLI emitted.
#
# WHAT IS ASSERTED:
#
#   A. stable  -- two computes over an unchanged tree agree.
#   B. EMIT    -- the tool emits a different fingerprint => the stamp moves.
#   C. BINARY  -- the tool's bytes move while its fingerprint does not => the
#                 stamp does NOT move. This is the property a binary-keyed fix
#                 fails, and the reason this file exists.
#   D. shape   -- `action.rs` still moves the stamp (Phase 214.J's original
#                 property, which the new term must not have displaced).
#   E. absent  -- no in-tree CLI is a STABLE marker, distinct from any present
#                 binary. "Assume unchanged" is the answer a freshness input
#                 must never give, and a silently-dropped term is that answer.
#   F. NEGATIVE CONTROL -- the pre-fix rule (sha256 of `action.rs` alone) is
#                 re-applied to the SAME trees and must NOT move in case B.
#                 Without it, A-E would also pass against a stamp that had been
#                 quietly reduced to something that moves for other reasons, and
#                 it is what proves the fixture reproduces the hazard rather
#                 than modelling something easier.
#
# The `nros` here is a shell script that prints a fingerprint, because that is
# the entire interface `nros_codegen_fingerprint` uses (`codegen-fingerprint` on
# stdout) and because building two real CLIs would put minutes on the fast line.
# Case C is why the stub matters: it is the only cheap way to hold the emitted
# bytes fixed while the binary's bytes move.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/build/codegen-stamp.sh
source "$REPO_ROOT/scripts/build/codegen-stamp.sh"

fails=0
ok()   { echo "  ok    $1"; }
fail() { echo "  FAIL  $1"; fails=$((fails + 1)); }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

ROOT="$TMP/root"
mkdir -p "$ROOT/packages/core/nros-core/src" "$ROOT/packages/cli/target/release"
printf 'pub trait RosAction {}\n' > "$ROOT/packages/core/nros-core/src/action.rs"

# `nros codegen-fingerprint` stub. $1 is the fingerprint it emits, $2 arbitrary
# padding that changes the FILE's bytes without changing what it prints.
write_stub() {
    local fp="$1" pad="${2-}"
    cat > "$ROOT/packages/cli/target/release/nros" <<EOF
#!/bin/sh
# pad: $pad
[ "\$1" = "codegen-fingerprint" ] || exit 2
echo "$fp"
EOF
    chmod +x "$ROOT/packages/cli/target/release/nros"
}

stamp() { NROS_REPO_DIR="$ROOT" nros_codegen_stamp_compute; }

# The rule this file replaces, applied to the same tree (case F).
prefix_rule() { sha256sum < "$ROOT/packages/core/nros-core/src/action.rs" | awk '{print $1}'; }

echo "codegen-stamp decision rule (issue 1018)"

# --- A: stable over an unchanged tree -------------------------------------
write_stub "fingerprint-one"
s0="$(stamp)"; s0b="$(stamp)"
if [ -n "$s0" ] && [ "$s0" = "$s0b" ]; then
    ok "A  two computes over an unchanged tree agree"
else
    fail "A  two computes over an unchanged tree agree ($s0 vs $s0b)"
fi

# --- B: the tool emits differently => the stamp moves ---------------------
write_stub "fingerprint-two"
s_emit="$(stamp)"
if [ "$s_emit" != "$s0" ]; then
    ok "B  a changed codegen fingerprint moves the stamp"
else
    fail "B  a changed codegen fingerprint moves the stamp (both $s0)"
fi

# --- F: the NEGATIVE CONTROL for B ---------------------------------------
# The pre-fix rule, on the very trees case B just distinguished.
p_before="$(prefix_rule)"
write_stub "fingerprint-one"; p_one="$(prefix_rule)"
write_stub "fingerprint-two"; p_two="$(prefix_rule)"
if [ "$p_one" = "$p_two" ] && [ "$p_before" = "$p_one" ]; then
    ok "F  NEGATIVE CONTROL: the pre-fix rule is blind to the same change"
else
    fail "F  NEGATIVE CONTROL: the pre-fix rule is blind to the same change"
fi

# --- C: the binary moves, the fingerprint does not => stamp holds ---------
# The 0835 budget property. Two DIFFERENT binaries (different sha256, so a
# different cache key and a real re-probe) emitting the same fingerprint.
write_stub "fingerprint-one" "rebuild-a"
h_a="$(sha256sum "$ROOT/packages/cli/target/release/nros" | awk '{print $1}')"
s_a="$(stamp)"
write_stub "fingerprint-one" "rebuild-b-different-bytes"
h_b="$(sha256sum "$ROOT/packages/cli/target/release/nros" | awk '{print $1}')"
s_b="$(stamp)"
if [ "$h_a" = "$h_b" ]; then
    fail "C  the two stub binaries must differ in bytes (fixture defect)"
elif [ "$s_a" = "$s_b" ]; then
    ok "C  a rebuilt binary with an unchanged fingerprint leaves the stamp still"
else
    fail "C  a rebuilt binary with an unchanged fingerprint leaves the stamp still ($s_a vs $s_b)"
fi

# --- D: `action.rs` still moves it ----------------------------------------
s_shape_before="$(stamp)"
printf 'pub trait RosAction { fn feedback(); }\n' > "$ROOT/packages/core/nros-core/src/action.rs"
s_shape_after="$(stamp)"
if [ "$s_shape_before" != "$s_shape_after" ]; then
    ok "D  a trait-surface edit still moves the stamp (Phase 214.J)"
else
    fail "D  a trait-surface edit still moves the stamp (Phase 214.J)"
fi

# --- E: absent CLI is a stable, distinct marker ---------------------------
present="$(stamp)"
rm -f "$ROOT/packages/cli/target/release/nros"
a1="$(stamp)"; a2="$(stamp)"
if [ -z "$a1" ]; then
    fail "E  an absent CLI still produces a stamp"
elif [ "$a1" != "$a2" ]; then
    fail "E  an absent CLI produces a STABLE stamp ($a1 vs $a2)"
elif [ "$a1" = "$present" ]; then
    fail "E  an absent CLI is distinguishable from a present one"
else
    ok "E  an absent CLI is a stable marker, distinct from a present binary"
fi

echo
if [ "$fails" -eq 0 ]; then
    echo "codegen-stamp: all checks passed"
    exit 0
fi
echo "codegen-stamp: $fails check(s) FAILED"
exit 1
