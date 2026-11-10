#!/usr/bin/env bash
# The native-example lint lane (`just linux check`, gate `check::linux-check`):
# `cargo fmt --check` + `cargo clippy` over every standalone example leaf a host
# can lint, each rendered by `nros configure` first (issue 1892). The per-leaf
# work lives in `lint-example-leaf.sh` (issue 1882); this script owns WHICH
# leaves and HOW they are fanned out, so the `just` lane and the gate share one
# spelling of both.
#
# Modes:
#
#   (none)                     every leaf, under one jobserver pool (or serial
#                              under an outer jobserver), then the leaf
#                              `target/` assertion.
#   NROS_GATE_LIST_UNITS=1     print one unit per leaf and exit (the nextest
#                              gate harness, phase-543 W2).
#   NROS_GATE_UNIT=<unit>      lint that one leaf, then the assertion.
#
# Why units (issue 2380): the 32 leaves are 32 cargo roots, and a target dir
# serves exactly ONE root (issue 0616), so each leaf compiles the nros stack on
# its own and none of that can be shared. Measured cold on a 32-core x86 host:
# 82 s for the whole lane idle, 831 s inside a loaded `ci gate`; 1,493-2,400 s
# on arm-a100 at load ~90, against the harness's 2400 s budget per TEST. One
# leaf is 11-83 s idle. As units, each leaf gets its own budget and nextest
# schedules them by CPU weight (`cpu:8` on the recipe) beside the other gates.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

# Exclude example trees that can't be host-clippy'd standalone (validated
# via the fixture-manifest / orchestration path instead, as
# multi-package-workspace already is):
#   - ws-sync templates: workspace msg pkgs (`extra_msgs`/`local_msgs`) only
#     resolve via the `[patch.crates-io]` block `nros configure` writes; cargo
#     can't find them on crates.io here.
#   - `examples/workspaces/*`: a multi-target workspace whose entry packages
#     are cross-compiled (esp32/threadx/nuttx/freertos/zephyr) — e.g.
#     `esp32_entry` configures `portable-atomic` single-core, which fails to
#     compile for the host x86_64 target.
# Tracked-file enumeration via the git index (no filesystem traversal;
# untracked build/target trees can never slow this down or leak in).
leaves() {
    git ls-files 'examples/**/Cargo.toml' | awk -F/ 'NF>=5' \
        | grep -vE '/zephyr/|/multi-package-workspace/|/templates/local-msg-package/|/templates/multi-node-workspace/|/templates/workspace-shadowing/|/workspaces/|esp32-c3-baremetal/rust/dds/|/mps2-an385-freertos/|/qemu-armv7a-nuttx/|/threadx-linux/|/rv-virt-threadx/|/px4/' \
        | python3 scripts/lib/drop_workspace_members.py \
        | sort | while read -r toml; do dirname "$toml"; done
}

# A leaf's unit name: its path under examples/ with `/` as `-`, the same
# spelling as its lint target dir (`lint-example-leaf.sh`).
unit_of() { local d="${1#examples/}"; printf '%s\n' "${d//\//-}"; }

mapfile -t all < <(leaves)
if [ "${#all[@]}" -eq 0 ]; then
    echo "lint-examples: no example leaf found; the enumeration is broken, which is not a pass" >&2
    exit 1
fi

if [ "${NROS_GATE_LIST_UNITS:-}" = 1 ]; then
    for d in "${all[@]}"; do unit_of "$d"; done
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
    for d in "${all[@]}"; do
        if [ "$(unit_of "$d")" = "$NROS_GATE_UNIT" ]; then
            bash scripts/build/lint-example-leaf.sh "$d"
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
    for d in "${all[@]}"; do bash scripts/build/lint-example-leaf.sh "$d"; done
else
    echo "Checking examples (jobserver fan-out)..."
    source scripts/build/jobserver-pool.sh
    # ONE self-contained line per leaf: a make recipe is one command per line.
    for d in "${all[@]}"; do
        printf 'bash scripts/build/lint-example-leaf.sh %s\n' "$d"
    done | nros_pool_run check-examples
fi
assert_no_leaf_target
echo "All examples check passed!"
