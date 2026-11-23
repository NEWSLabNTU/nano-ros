#!/bin/bash
# tests/census-images-compile-check-rows-tests.sh -- issue 2911
#
# `fixtures-manifest.py census-images` is the census pre-pass's list of images
# to take a census for. It read only the `[[workspace_fixture]]` table, so a
# `[[compile_check_fixture]]` row configured by `nros build <bringup>:<image>`
# (`west_nros_image`) never had its census taken, and
# `west_owned_entry_census_flat` / `_nested`, whose image builds an owned entry
# sized by its census, refused at configure on every fresh tree. Checked
# against the real manifest:
#
#   1. every compile-check row the west builder configures through `nros
#      build` (`compile_check_nros_image`, read from the same record field
#      `west-fixtures.sh` reads) has its (dir, image) listed: the listing and
#      the configure share one predicate, so a new row cannot be missed;
#   2. both issue-2862 census rows are among them;
#   3. `--platform zephyr` (the Zephyr lane's pre-pass) keeps them, and
#      `--platform freertos` drops them;
#   4. `--id` narrows to one compile-check row;
#   5. each (dir, image) is listed once.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
manifest_py="$ROOT/scripts/build/fixtures-manifest.py"
# shellcheck source=scripts/lib/grep-q.sh
source "$ROOT/scripts/lib/grep-q.sh"
fail=0

listed() {
    local rc=0
    nros_grep_q -x -F -- "$1" <<< "$2" || rc=$?
    return "$rc"
}

all="$(python3 "$manifest_py" census-images)"
zephyr="$(python3 "$manifest_py" census-images --platform zephyr)"
freertos="$(python3 "$manifest_py" census-images --platform freertos)"

# The rows the west builder runs `nros build` on: field 12 (`west_nros_image`
# slot, filled by `compile_check_nros_image`) of the records it reads.
records="$(python3 "$manifest_py" list-compile-checks --builder west-build;
    python3 "$manifest_py" list-compile-checks --builder west-configure)"
want="$(awk -F $'\x1f' '$12 != "" { print $3 "\t" $12 }' <<< "$records")"
if [ -z "$want" ]; then
    echo "FAIL 1: no west compile-check row names an image; the probe has nothing to check" >&2
    fail=1
else
    missing=""
    while IFS= read -r line; do
        listed "$line" "$all" || missing+="$line"$'\n'
    done <<< "$want"
    if [ -z "$missing" ]; then
        echo "ok   1: census-images lists all $(grep -c . <<< "$want") compile-check row(s) configured by \`nros build\`"
    else
        echo "FAIL 1: census-images leaves out compile-check rows configured by \`nros build\`:" >&2
        sed 's/^/       /' <<< "$missing" >&2
        fail=1
    fi
fi

for row in west_owned_entry_census_flat west_owned_entry_census_nested; do
    line="packages/testing/nros-tests/fixtures/$row"$'\t'"talker_bringup:zephyr"
    if listed "$line" "$all"; then
        echo "ok   2: $row is listed"
    else
        echo "FAIL 2: $row (talker_bringup:zephyr) is not listed" >&2
        fail=1
    fi
    if listed "$line" "$zephyr" && ! listed "$line" "$freertos"; then
        echo "ok   3: $row is in the zephyr pre-pass and not in the freertos one"
    else
        echo "FAIL 3: $row: --platform zephyr / freertos narrowing is wrong" >&2
        fail=1
    fi
done

one="$(python3 "$manifest_py" census-images --id west_owned_entry_census_nested)"
if [ "$one" = "packages/testing/nros-tests/fixtures/west_owned_entry_census_nested"$'\t'"talker_bringup:zephyr" ]; then
    echo "ok   4: --id narrows to the one compile-check row"
else
    echo "FAIL 4: --id west_owned_entry_census_nested listed:" >&2
    sed 's/^/       /' <<< "${one:-<nothing>}" >&2
    fail=1
fi

dups="$(sort <<< "$all" | uniq -d)"
if [ -z "$dups" ]; then
    echo "ok   5: every (dir, image) is listed once"
else
    echo "FAIL 5: listed more than once:" >&2
    sed 's/^/       /' <<< "$dups" >&2
    fail=1
fi

exit "$fail"
