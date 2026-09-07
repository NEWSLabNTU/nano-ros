#!/usr/bin/env bash
#
# issue 0460 — the capability slot constants must match the services actually
# registered.
#
# # The failure this prevents
#
# `nros::main!` sizes an entry's executor from the model's callback count. A
# CAPABILITY registers services the model never mentions: `[lifecycle]` five
# (REP-2002), `[param_services]` six. Sized without them, an entry that
# declares either one fails inside `register_*_services()` — and on Zephyr the
# generated `rust_main` dropped that `Result`, so the image printed nothing
# after "Network ready" and three `entry_matrix` cells looked like hangs.
#
# The counts therefore live in `executor_sizing` (the proc-macro can only read
# consts from a crate it depends on), while the services live in `nros-node`,
# which does not depend on it — so nothing in the type system ties them
# together. A field added to either server struct without bumping its constant
# silently under-sizes every entry declaring that capability.
#
# # What it checks
#
# The number of SERVER fields in each `*ServiceServers` struct equals its
# constant. Field-counting is crude but it is the thing that changes: the
# structs are lists of one server per service.
#
# phase-426 W1 — "server field", not "field". A server is always typed through
# the file's own generic alias (`ParamServer<Svc>` / `LcSrv<Svc>`), so a field
# whose type has no `<` is not a service and must not be counted: the
# parameter set gained a `node: NodeKey` (which node's parameters these six
# serve) and the gate read it as a seventh service. Counting every field made
# the gate's REACH wider than the rule it enforces, which is issue 0196's
# shape — a struct with any bookkeeping field would have failed it.

set -euo pipefail
cd "$(dirname "$0")/.."

fail=0

count_fields() {
    # SERVER fields between `pub struct <name> {` and the closing brace: a
    # field whose type is the file's generic server alias (`ParamServer<Svc>`,
    # `LcSrv<Svc>`). A field with a non-generic type is bookkeeping, not a
    # service — see the header note.
    awk -v pat="pub struct $1 [{]" '
        $0 ~ pat {inside=1; next}
        inside && /^\}/ {exit}
        inside && /^[[:space:]]*[a-z_]+:[[:space:]]*[A-Za-z_][A-Za-z0-9_]*</ {n++}
        END {print n+0}
    ' "$2"
}

const_of() {
    grep -oE "pub const $1: usize = [0-9]+" \
        packages/core/nros-orchestration-ir/src/executor_sizing.rs \
        | grep -oE '[0-9]+$'
}

check() {
    local struct="$1" file="$2" const_name="$3"
    [ -f "$file" ] || { echo "[FAIL] missing $file" >&2; fail=1; return; }
    local fields want
    fields="$(count_fields "$struct" "$file")"
    want="$(const_of "$const_name")"
    if [ -z "$want" ]; then
        echo "[FAIL] $const_name not found in executor_sizing.rs" >&2
        fail=1
        return
    fi
    if [ "$fields" != "$want" ]; then
        echo "[FAIL] $struct has $fields service(s) but $const_name = $want" >&2
        echo "       An entry declaring this capability is sized for $want slots" >&2
        echo "       and will fail inside its register_*_services() call." >&2
        fail=1
    fi
}

lc="$(git grep -l 'pub struct LifecycleServiceServers' -- packages/core/nros-node/src | head -1)"
pm="$(git grep -l 'pub struct ParameterServiceServers' -- packages/core/nros-node/src | head -1)"
check LifecycleServiceServers "$lc" LIFECYCLE_SERVICE_SLOTS
check ParameterServiceServers "$pm" PARAM_SERVICE_SLOTS

if [ "$fail" != 0 ]; then
    echo "" >&2
    echo "  Update the constant in" >&2
    echo "  packages/core/nros-orchestration-ir/src/executor_sizing.rs to match." >&2
    exit 1
fi

echo "capability-slot-counts OK — lifecycle/param service counts match their sizing constants."
