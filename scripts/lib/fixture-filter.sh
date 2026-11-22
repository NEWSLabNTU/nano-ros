# shellcheck shell=bash
# Sourced, not executed — hence no shebang.
#
# Does `NROS_ZEPHYR_FIXTURE_FILTER` select this row? — issue 1325.
#
# Both Zephyr builders (`scripts/build/zephyr-fixture-leaves.sh` for the
# `[[fixture]]` west leaves, `scripts/build/west-fixtures.sh` for the
# `[[compile_check_fixture]]` rows) matched the filter against ONE joined
# string, `"board build_dir src conf_files id"`. That string had two defects,
# both silent in the expensive direction (too many leaves selected, never too
# few):
#
#   - the leaves' conf list carries ABSOLUTE paths (the board overlay tail), so
#     the checkout's own directory name was in every row's haystack: in a
#     worktree named `448-xrce`, `--filter xrce` selected all 57 leaves.
#   - `.*` in a bash `=~` crosses the spaces between fields, so
#     `build-(c|cpp)-.*-xrce` paired a `build-c-` in the build dir with an
#     `-xrce` somewhere later in the line.
#
# So the regex is tested against each FIELD on its own, and a `;`-separated
# list field (the conf files) against each ENTRY on its own. A value under the
# checkout root is matched repo-relative, so where the checkout lives is never
# part of what a filter can see. The row is selected when ANY piece matches,
# which keeps every documented spelling working (`build-rust-.*-zenoh`,
# `/xrce$` on the id, `zephyr_self_pkg_sibling`).
#
#   nros_fixture_filter_selects <regex> <checkout-root> <field>...

nros_fixture_filter_selects() {
    local filter="$1" root="$2" field piece
    shift 2
    local -a pieces
    for field in "$@"; do
        IFS=';' read -r -a pieces <<< "$field"
        for piece in "${pieces[@]}"; do
            piece="${piece#"$root"/}"
            [ -n "$piece" ] || continue
            if [[ "$piece" =~ $filter ]]; then
                return 0
            fi
        done
    done
    return 1
}

# nros_fixture_id_selects <row-id>
#
# Issue 2835 — `NROS_FIXTURE_ID=<id>` is the sweep-wide one-row narrowing
# (`scripts/build/fixture-id-guard.sh`): every stage a platform recipe runs
# must honour it. The two Zephyr west builders read only
# NROS_ZEPHYR_FIXTURE_FILTER, so `NROS_FIXTURE_ID=<id> just zephyr
# build-fixtures`, the refusal hint's own spelling, built every leaf. True when
# the variable is unset or names exactly this row.
nros_fixture_id_selects() {
    [ -z "${NROS_FIXTURE_ID:-}" ] || [ "$1" = "$NROS_FIXTURE_ID" ]
}

# nros_fixture_narrowing — the narrowings in force, for a "matched nothing"
# message; empty when none is.
nros_fixture_narrowing() {
    local out=""
    [ -z "${NROS_ZEPHYR_FIXTURE_FILTER:-}" ] || out="NROS_ZEPHYR_FIXTURE_FILTER=$NROS_ZEPHYR_FIXTURE_FILTER"
    [ -z "${NROS_FIXTURE_ID:-}" ] || out="${out:+$out }NROS_FIXTURE_ID=$NROS_FIXTURE_ID"
    printf '%s' "$out"
}

