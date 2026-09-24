#!/usr/bin/env bash
# Issue 1468 — compile `nros-node` with each MODULE-GATING feature ON ITS OWN.
#
# A feature combination nothing builds is a combination nothing checks. The
# workspace `cargo check` two lines up in `check::compile-smoke` UNIFIES
# features across every member, so `nros-node` is built there with whatever the
# union of the workspace turns on: `param-services` and `lifecycle-services`
# are both on, always, and a module reaching across the gate between them is
# invisible. That is exactly how phase-461 W2 landed an unconditional
# `use crate::parameter_services::{…}` inside `lifecycle_services` — green on
# its own pull request, and `Build rust core fixtures` red on `host-tests`
# minutes after the merge, because `examples/native/rust/lifecycle-node` picks
# `lifecycle-services` and not `param-services`.
#
# The feature list is DERIVED from `lib.rs`, not written here: a hand-kept list
# is only as complete as whoever last edited it, and a NEW gated module is the
# case this gate exists for. Every `#[cfg(… feature = "X" …)]` immediately
# above a `mod`/`pub mod` declaration contributes X.
#
# Scope is what the rule can afford, and the reach is stated rather than
# implied: this builds ONE crate for the HOST. `nros-node` is where the class
# has bitten (issues 1175, 1177, 1468) and it is the crate every image links.
# The same shape exists in the board crates (`rtic` / `board-entry` /
# `ethernet` / `xrce-transport`) and needs a cross toolchain AND a provisioned
# `zenoh-pico`, so it belongs to `check::workspace-embedded`'s tier, not to a
# pull-request lane. Issue 1177 still owns the question of which tier the
# per-feature rows as a whole live in.
set -euo pipefail

cd "${NROS_REPO_DIR:-$(git rev-parse --show-toplevel)}"

LIB="packages/core/nros-node/src/lib.rs"

# Every feature named in the `#[cfg(...)]` attribute that immediately precedes
# a module declaration, deduplicated.
#
# Remembering the LAST attribute and clearing it on any other line is what
# keeps a `#[cfg]` on some unrelated `const` from being read as a module's
# gate; the selftest below is about exactly that.
derive_features() {
    awk '
        /^#\[cfg\(/ { cfg = $0; next }
        /^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z_]+[[:space:]]*;/ {
            while (match(cfg, /feature = "[^"]+"/)) {
                f = substr(cfg, RSTART + 11, RLENGTH - 12)
                print f
                cfg = substr(cfg, RSTART + RLENGTH)
            }
            cfg = ""
            next
        }
        { cfg = "" }
    ' "$1" | sort -u
}

# The negative control, on the NORMAL path: a synthetic `lib.rs` whose right
# answer is known. Without it the derivation could stop matching — a `mod` line
# spelled slightly differently, an attribute shape awk no longer recognises —
# and this gate would then check NOTHING while printing success, which is the
# vacuous-gate shape the whole file argues against.
selftest() {
    local tmp expect got
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN
    {
        printf '#[cfg(feature = "alpha")]\n'
        printf 'pub mod alpha_mod;\n'
        printf '\n'
        printf '#[cfg(all(feature = "beta", any(has_rmw, test)))]\n'
        printf 'mod beta_mod;\n'
        printf '\n'
        printf '#[cfg(any(feature = "gamma", feature = "delta"))]\n'
        printf 'pub(crate) mod both_mod;\n'
        printf '\n'
        printf 'pub mod ungated_mod;\n'
        printf '\n'
        printf '// A cfg that gates something that is NOT a module must not count.\n'
        printf '#[cfg(feature = "epsilon")]\n'
        printf 'pub const NOT_A_MODULE: usize = 1;\n'
        printf 'pub mod after_the_const;\n'
    } > "$tmp/lib.rs"

    expect="$(printf 'alpha\nbeta\ndelta\ngamma\n')"
    got="$(derive_features "$tmp/lib.rs")"
    if [ "$got" != "$expect" ]; then
        echo "check-feature-gated-modules SELFTEST FAILED: the derivation does not read lib.rs" >&2
        echo "  expected: $(echo "$expect" | tr '\n' ' ')" >&2
        echo "  got:      $(echo "$got" | tr '\n' ' ')" >&2
        return 1
    fi
}

selftest

[ -f "$LIB" ] || { echo "check-feature-gated-modules: $LIB not found" >&2; exit 1; }

feats="$(derive_features "$LIB")"
[ -n "$feats" ] || { echo "check-feature-gated-modules: derived NO features from $LIB" >&2; exit 1; }

# The floor every row shares: a host build with a backend, so `has_rmw` is on
# and the service modules are reachable at all. `rmw-cffi` is itself in the
# derived list and is therefore also checked alone, against this floor minus
# itself — which is what the floor being a separate variable buys.
base="std,alloc"

rc=0
for f in $feats; do
    echo "  - nros-node: $f alone (over $base)"
    if ! cargo check --quiet -p nros-node --no-default-features \
            --features "$base,rmw-cffi,$f"; then
        echo "check-feature-gated-modules: nros-node does not compile with only \`$f\`" >&2
        echo "  A module gated on one feature is reaching into a module gated on another." >&2
        echo "  The shared item belongs in a module below BOTH gates (issue 1468)." >&2
        rc=1
    fi
done

if [ "$rc" -ne 0 ]; then
    exit 1
fi
echo "check-feature-gated-modules: selftest ok; every module-gating feature of nros-node compiles alone."
