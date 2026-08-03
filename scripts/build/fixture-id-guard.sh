#!/usr/bin/env bash
# Issue 0406 — what a fixture builder says when its id filter matched no rows.
#
# Before this, every builder answered the same way: exit 0, having built
# nothing. `fixtures-build.sh native rust --id workspace-rust-native-realtime`
# returned in 0.03s with no output — a real id, a real platform, a real lang,
# and no work, because that id names a `[[workspace_fixture]]` and that script
# lists only `[[fixture]]` rows. Same clean exit for a typo'd id or platform.
# The stamp machinery (issue 0393) then certifies a build that never happened,
# which is the failure shape of 0351 and 0196.
#
# The fix cannot be "no rows = error": sweeps hit empty coordinates all the
# time (`threadx-linux/mixed` has 0 rows, and the per-platform recipes iterate
# all four languages). What separates an error from a normal empty pass is HOW
# the filter arrived, and the two spellings already differ in exactly that way:
#
#   --id <id>            a human targeting THIS builder. Nothing else will run.
#                        Zero rows means the invocation was wrong -> FAIL.
#
#   NROS_FIXTURE_ID=<id> a sweep-wide narrowing that crosses builders (read by
#                        workspace-fixtures-build.sh and
#                        compile-check-fixtures.sh, and by every stage a
#                        platform recipe runs). Some stages legitimately match
#                        nothing -> NOTE and pass. Unless the id exists in no
#                        table at all, which no stage can ever build -> FAIL.
#
# So the loudness is keyed on the spelling, and the spellings keep distinct
# meanings instead of being merged into one that cannot express both.

# nros_fixture_id_no_match <id> <source> <kind> <platform> <lang> [rmw]
#
#   source: "flag" (--id) or "env" (NROS_FIXTURE_ID)
#   kind:   the table THIS builder reads — fixture | workspace_fixture |
#           compile_check_fixture
#
# Returns 0 if the caller should proceed with an empty row set (a benign sweep
# miss); exits non-zero otherwise. Never returns when it decides to fail.
nros_fixture_id_no_match() {
    local id="$1" source="$2" kind="$3" platform="$4" lang="${5:-}" rmw="${6:-}"
    local where other_kinds=() same_kind_coords=() guard_dir

    # Resolve the manifest script from THIS file's location, not the caller's
    # cwd — the three builders run from different directories.
    guard_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local guard_root="${guard_dir%/scripts/build}"
    where="$(python3 "$guard_dir/fixtures-manifest.py" \
        --manifest "$guard_root/examples/fixtures.toml" describe-id --id "$id" 2>/dev/null)"

    local other_platform="" other_lang=""
    while IFS=$'\x1f' read -r k p l r; do
        [ -n "$k" ] || continue
        if [ "$k" = "$kind" ]; then
            same_kind_coords+=("platform=${p:-–} lang=${l:-–} rmw=${r:-–}")
        else
            other_kinds+=("$k")
            # Remember the row's OWN coordinates: the hint must name the
            # platform/lang that actually builds it, not the ones this caller
            # happened to pass (a freertos row hinted as `native rust` sends
            # the reader straight into a second empty build).
            [ -n "$other_platform" ] || other_platform="$p"
            [ -n "$other_lang" ] || other_lang="$l"
        fi
    done <<< "$where"

    # Case 1 — the id exists in no table. Always fatal: no builder, in any
    # sweep, can ever match it. This is the typo.
    if [ -z "$where" ]; then
        {
            echo "fixtures: no row anywhere carries id '${id}'."
            echo "          Searched every table in examples/fixtures.toml:"
            echo "          [[fixture]], [[workspace_fixture]], [[compile_check_fixture]]."
            echo "          Check the spelling:  grep -n '${id}' examples/fixtures.toml"
        } >&2
        exit 2
    fi

    # Case 2 — the id belongs to a different table.
    if [ "${#same_kind_coords[@]}" -eq 0 ]; then
        local uniq
        uniq="$(printf '%s\n' "${other_kinds[@]}" | sort -u | tr '\n' ' ')"
        if [ "$source" = "env" ]; then
            # A sweep narrowing crossing builders: this stage has nothing to do,
            # which is correct and not an error. Say so once, so the run does
            # not read as "built the whole stage".
            echo "fixtures: NROS_FIXTURE_ID='${id}' names a ${uniq%% *}; this ${kind} stage builds nothing."
            return 0
        fi
        {
            echo "fixtures: id '${id}' is a ${uniq%% *}, but this builder reads ${kind} rows."
            echo "          Build it with:"
            nros_fixture_id_builder_hint \
                "$id" "${uniq%% *}" "${other_platform:-$platform}" "${other_lang:-$lang}"
        } >&2
        exit 2
    fi

    # Case 3 — right table, wrong coordinates.
    if [ "$source" = "env" ]; then
        echo "fixtures: NROS_FIXTURE_ID='${id}' is not a ${kind} for platform=${platform} lang=${lang}; nothing to build here."
        return 0
    fi
    {
        echo "fixtures: id '${id}' is a ${kind}, but not for platform=${platform} lang=${lang}${rmw:+ rmw=${rmw}}."
        echo "          It is declared for:"
        printf '            %s\n' "${same_kind_coords[@]}"
    } >&2
    exit 2
}

# nros_fixture_require_known_platform <platform>
#
# Issue 0406's third case: `fixtures-build.sh natve rust` (typo) matched no
# rows and exited 0 like a normal empty coordinate. An empty (platform, lang)
# IS routine — but only for a platform that exists. A platform naming no row
# anywhere is a typo, and every platform the recipes pass is in the manifest,
# so rejecting the rest costs nothing.
nros_fixture_require_known_platform() {
    local platform="$1" guard_dir guard_root known
    guard_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    guard_root="${guard_dir%/scripts/build}"
    known="$(python3 "$guard_dir/fixtures-manifest.py" \
        --manifest "$guard_root/examples/fixtures.toml" list-platforms 2>/dev/null)"
    [ -n "$known" ] || return 0  # manifest unreadable — not this guard's job
    if ! grep -qxF "$platform" <<< "$known"; then
        {
            echo "fixtures: unknown platform '${platform}' — no row in examples/fixtures.toml declares it."
            echo "          Known platforms:"
            echo "$known" | sed 's/^/            /'
        } >&2
        exit 2
    fi
}

# The invocation that WOULD build this id, printed as part of a failure so the
# next command is copy-pasteable rather than inferred.
nros_fixture_id_builder_hint() {
    local id="$1" kind="$2" platform="$3" lang="$4"
    case "$kind" in
        fixture)
            echo "            scripts/build/fixtures-build.sh ${platform} ${lang} --id ${id}"
            ;;
        workspace_fixture)
            echo "            NROS_FIXTURE_ID=${id} scripts/build/workspace-fixtures-build.sh ${platform} ${lang}"
            echo "          or, for a platform needing SDK env (freertos/nuttx/zephyr/threadx):"
            echo "            NROS_FIXTURE_ID=${id} just <platform> build-fixtures"
            ;;
        compile_check_fixture)
            echo "            NROS_FIXTURE_ID=${id} bash scripts/build/compile-check-fixtures.sh"
            ;;
        *)
            echo "            (unknown row kind ${kind})"
            ;;
    esac
}
