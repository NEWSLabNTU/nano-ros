#!/usr/bin/env bash
# The native-example lint lane (`just linux check`, gate `check::linux-check`):
# `cargo fmt --check` + `cargo clippy` over every example Rust package a host
# can lint, each rendered by `nros configure` first (issue 1892). A standalone
# leaf is linted by `lint-example-leaf.sh` (issue 1882), a one-node witness
# workspace's node packages by `lint-example-witness.sh` (issue 2595). This
# script owns HOW the units are fanned out, so the `just` lane and the gate
# share one spelling; WHICH units, and their coverage proof, are
# `scripts/lib/example_lint_units.py`.
#
# Modes:
#
#   (none)                     every unit, under one jobserver pool (or serial
#                              under an outer jobserver), then the leaf
#                              `target/` assertion.
#   NROS_GATE_LIST_UNITS=1     print one unit per leaf or witness and exit (the
#                              nextest gate harness, phase-543 W2).
#   NROS_GATE_UNIT=<unit>      lint that one unit, then the assertion.
#
# Why units (issue 2380): every leaf and every witness package is its own cargo
# root (32 leaves when this was measured), and a target dir
# serves exactly ONE root (issue 0616), so each leaf compiles the nros stack on
# its own and none of that can be shared. Measured cold on a 32-core x86 host:
# 82 s for the whole lane idle, 831 s inside a loaded `ci gate`; 1,493-2,400 s
# on arm-a100 at load ~90, against the harness's 2400 s budget per TEST. One
# leaf is 11-83 s idle. As units, each leaf gets its own budget and nextest
# schedules them by CPU weight (`cpu:8` on the recipe) beside the other gates.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

# WHICH packages, and the proof that every tracked example package is linted
# by a unit or named by an exclusion with its reason, live in
# `scripts/lib/example_lint_units.py` (issue 2595: a witness workspace's
# packages left this list and no lane took them, 32 leaves to 1). Its listing
# FAILS when a package falls through, so the gate's own unit listing does too.
units_file="$(mktemp "${TMPDIR:-/tmp}/nros_lint_units.XXXXXX")"
trap 'rm -f "$units_file"' EXIT
if ! python3 scripts/lib/example_lint_units.py list >"$units_file"; then
    echo "lint-examples: listing the example lint units failed" >&2
    exit 1
fi
kinds=(); names=(); dirs=()
while IFS=$'\t' read -r k u d; do kinds+=("$k"); names+=("$u"); dirs+=("$d"); done <"$units_file"

# One unit's command line, for both the gate's single unit and the pool.
unit_cmd() {
    case "$1" in
        leaf) printf 'bash scripts/build/lint-example-leaf.sh %s\n' "$2" ;;
        witness) printf 'bash scripts/build/lint-example-witness.sh %s\n' "$2" ;;
    esac
}

if [ "${NROS_GATE_LIST_UNITS:-}" = 1 ]; then
    printf '%s\n' "${names[@]}"
    exit 0
fi

# phase-336 — resolve the profile ONCE here: `lint-example-leaf.sh` runs in
# pool units, where the cargo.sh helpers are not defined. An exported variable
# crosses; a function does not.
source scripts/build/cargo.sh
export NROS_EXAMPLE_PROFILE_FLAGS="$(nros_cargo_profile_arg_string)"
# issue 0624 — where this lane's cargo artifacts go, resolved once for the same
# reason. Without it, `cd <leaf> && cargo clippy` writes `<leaf>/target/`, the
# population issue 0488 declared empty and gated.
source scripts/build/build-root.sh
export NROS_EXAMPLE_LINT_DIR="$(nros_build_dir "$NROS_KIND_EXAMPLE_LINT")"
NIGHTLY="${NIGHTLY:-$(awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml)}"
export NIGHTLY

# issue 0624 — assert the leaf `target/` population is still empty at the end
# of the lane that used to repopulate it, so a writer added here fails in the
# same run rather than the next one.
assert_no_leaf_target() { python3 scripts/check-example-leaf-target-dirs.py; }

if [ -n "${NROS_GATE_UNIT:-}" ]; then
    for i in "${!names[@]}"; do
        if [ "${names[$i]}" = "$NROS_GATE_UNIT" ]; then
            bash -c "$(unit_cmd "${kinds[$i]}" "${dirs[$i]}")"
            assert_no_leaf_target
            exit 0
        fi
    done
    echo "lint-examples: no unit named \`$NROS_GATE_UNIT\` (NROS_GATE_LIST_UNITS=1 lists them)" >&2
    exit 1
fi

# Fan-out choice.
#
#   NROS_JOBSERVER=1 / NROS_INHERIT_JOBSERVER=1  an OUTER pool already owns the
#       tokens. A serial walk still saturates the machine, because cargo is
#       itself a jobserver client; fanning out on top would double-count the
#       budget (`nros_pool_available` refuses to nest for the same reason).
#
# Otherwise the units go to `nros_pool_run` (issue 0466: under the jobserver,
# not GNU parallel). Every unit is `cargo clippy`, and cargo given an inherited
# jobserver draws its rustc slots from it (rust-lang/cargo#4110), so N leaves
# share ONE token set. `nros_pool_run` degrades to a serial walk by itself when
# a pool is unavailable (no pinned make 4.4) — same commands, same order.
if [ "${NROS_JOBSERVER:-}" = "1" ] || [ "${NROS_INHERIT_JOBSERVER:-}" = "1" ]; then
    echo "Checking examples (serial; outer jobserver owns the tokens)..."
    for i in "${!names[@]}"; do bash -c "$(unit_cmd "${kinds[$i]}" "${dirs[$i]}")"; done
else
    echo "Checking examples (jobserver fan-out)..."
    source scripts/build/jobserver-pool.sh
    # ONE self-contained line per leaf: a make recipe is one command per line.
    for i in "${!names[@]}"; do unit_cmd "${kinds[$i]}" "${dirs[$i]}"; done \
        | nros_pool_run check-examples
fi
assert_no_leaf_target
echo "All examples check passed!"
