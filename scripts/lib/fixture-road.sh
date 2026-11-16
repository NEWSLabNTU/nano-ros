#!/usr/bin/env bash
# The road a `[[workspace_fixture]]` row builds on (issue 2463): its authored
# `builder` (the record's 15th column) when it states one, else its language
# (`rust` -> cargo, anything else -> cmake).
#
# The ONE bash spelling. Its twins are `row_road` in
# `scripts/build/workspace-fixture-units.py` and `road` in
# `packages/testing/nros-tests/src/fixtures/migrated_leaf.rs`;
# `check-workspace-fixture-road` holds the bash and Python ones to each other
# over every manifest row, and the Rust one has a unit test over the same
# table. A hand-written copy of this in `workspace-fixtures-build.sh` once
# tested the variable it was assigning, so every row with no `builder`
# stopped at `road: unbound variable`.
#
# Usage: nros_ws_row_road <lang> <builder>   (prints `cargo` or `cmake`)
nros_ws_row_road() {
    local lang="${1-}" builder="${2-}"
    if [ -n "$builder" ]; then
        printf '%s\n' "$builder"
    elif [ "$lang" = rust ]; then
        printf 'cargo\n'
    else
        printf 'cmake\n'
    fi
}
