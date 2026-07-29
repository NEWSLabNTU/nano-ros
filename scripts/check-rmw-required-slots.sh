#!/usr/bin/env bash
#
# Issues 0332 + 0349 — keep the RMW vtable's "required" list and its
# panic-on-dispatch sites in lockstep.
#
# Two failure modes, one already shipped each way:
#
#   * A slot the runtime `.expect()`s but that `first_missing_vtable_slot` does
#     NOT require -> a backend registers fine and PANICS mid-spin, on a no_std
#     target. That is issue 0332.
#   * A slot required but never `.expect()`ed -> an OPTIONAL capability treated
#     as mandatory, refusing an otherwise working backend. That is issue 0349:
#     three QoS-event/liveliness slots were on the required list, so the xrce
#     backend (which NULLs all three deliberately) could not register at all.
#
# The invariant is an equality, so this gate checks it in both directions. A
# slot that should be optional is made optional by giving it a typed
# `TransportError::Unsupported` at the point of use instead of an `.expect()` —
# which is precisely what removes it from the left-hand set here.

set -euo pipefail
cd "$(dirname "$0")/.."

SRC="packages/core/nros-rmw-cffi/src/lib.rs"
[ -f "$SRC" ] || { echo "ERROR: $SRC not found" >&2; exit 1; }

# Slots dispatched via `.expect("rmw vtable: <slot>")`.
expected="$(grep -oE 'expect\("rmw vtable: [a-z_]+"' "$SRC" \
    | sed -E 's/.*: ([a-z_]+)"/\1/' | sort -u)"

# Slots listed in first_missing_vtable_slot's `require!(...)`.
required="$(awk '
    /^fn first_missing_vtable_slot/ { infn = 1 }
    infn && /require!\(/ { inlist = 1; next }
    inlist && /^\s*\);/ { inlist = 0; infn = 0 }
    inlist { print }
' "$SRC" | grep -oE '^\s*[a-z_]+,' | tr -d ' ,' | sort -u)"

if [ -z "$expected" ] || [ -z "$required" ]; then
    echo "ERROR: could not extract slot lists from $SRC — has the shape changed?" >&2
    exit 1
fi

fail=0

missing_from_required="$(comm -23 <(echo "$expected") <(echo "$required"))"
if [ -n "$missing_from_required" ]; then
    echo "ERROR: slot(s) .expect()ed on dispatch but NOT required at registration:" >&2
    echo "$missing_from_required" | sed 's/^/       /' >&2
    echo "       A backend omitting one registers cleanly and panics mid-spin" >&2
    echo "       (issue 0332). Either require it, or give it a typed" >&2
    echo "       TransportError::Unsupported at the point of use." >&2
    fail=1
fi

missing_from_expected="$(comm -13 <(echo "$expected") <(echo "$required"))"
if [ -n "$missing_from_expected" ]; then
    echo "ERROR: slot(s) required at registration but never .expect()ed:" >&2
    echo "$missing_from_expected" | sed 's/^/       /' >&2
    echo "       Nothing depends on them being present, so requiring them only" >&2
    echo "       refuses otherwise-working backends (issue 0349 — this is how" >&2
    echo "       the xrce backend became unregistrable)." >&2
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    echo "RMW required-slot lockstep gate FAILED." >&2
    exit 1
fi

echo "RMW required slots match the .expect()ed dispatch sites ($(echo "$required" | wc -l) slots)."
