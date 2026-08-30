#!/usr/bin/env bash
# Issue 0406 — assert that a fixture builder narrowed to a non-matching id
# fails, and that the cases which must stay green stay green.
#
# The bug this locks out is a SILENT one: exit 0 having built nothing. Nothing
# downstream can notice that, which is why it needs a gate rather than a
# convention. The distinction being pinned is that loudness is keyed on the
# SPELLING of the filter, not on the emptiness alone:
#
#   --id (flag) = this invocation targets this builder -> empty is fatal
#   NROS_FIXTURE_ID (env) = a sweep-wide narrowing crossing builders -> empty
#                           is normal, EXCEPT when the id exists nowhere
#
# Most cases exercise the shared guard directly (no CLI, no SDK, no build), and
# one runs a real builder end to end so the gate also proves it is wired in.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# shellcheck source=scripts/build/fixture-id-guard.sh
source scripts/build/fixture-id-guard.sh

fails=0

# Ids picked from the manifest at run time — hardcoding one would rot the day
# it is renamed, and this gate would then pass by testing nothing.
ws_id="$(python3 scripts/build/fixtures-manifest.py list-workspaces --platform linux --lang rust \
    | head -1 | cut -d$'\x1f' -f1)"
fx_id="$(python3 - <<'PY'
import re
s = open("examples/fixtures.toml").read()
parts = re.split(r'^\[\[(\w+)\]\]\s*$', s, flags=re.M)
for i in range(1, len(parts), 2):
    if parts[i] == "fixture":
        m = re.search(r'^id\s*=\s*"([^"]+)"', parts[i + 1], re.M)
        if m:
            print(m.group(1))
            break
PY
)"

if [ -z "$ws_id" ] || [ -z "$fx_id" ]; then
    echo "check-fixture-id-guard: could not sample ids from examples/fixtures.toml" >&2
    exit 1
fi

# expect_rc <want-rc> <label> <command...>
#
# The guard EXITS rather than returning, so each case runs in a subshell.
expect_rc() {
    local want="$1" label="$2"
    shift 2
    local out rc
    out="$("$@" 2>&1)"
    rc=$?
    if [ "$rc" -ne "$want" ]; then
        echo "  FAIL  ${label}: expected rc=${want}, got rc=${rc}"
        echo "        output: ${out}"
        fails=$((fails + 1))
        return
    fi
    # A pass that says nothing is the bug itself — every case must speak.
    if [ -z "$out" ]; then
        echo "  FAIL  ${label}: rc=${rc} as expected, but printed NOTHING"
        fails=$((fails + 1))
        return
    fi
    echo "  ok    ${label} (rc=${rc})"
}

guard() { # <id> <source> <kind> <platform> <lang>
    ( nros_fixture_id_no_match "$1" "$2" "$3" "$4" "$5" )
}

echo "check-fixture-id-guard: an id filter that matches nothing"

# The case that started it: a real id, aimed at the builder that cannot build
# it. Fatal, because nothing else in this invocation will build it either.
expect_rc 2 "flag + id of another kind is fatal" \
    guard "$ws_id" flag fixture linux rust

# A typo can never match anywhere, in any stage, under any spelling.
expect_rc 2 "flag + id that exists nowhere is fatal" \
    guard "no-such-fixture-id-anywhere" flag fixture linux rust
expect_rc 2 "env + id that exists nowhere is fatal" \
    guard "no-such-fixture-id-anywhere" env workspace_fixture linux rust

# The sweep cases: a narrowing that crosses builders leaves some stages with
# nothing to do. That is correct, and must not fail the run.
expect_rc 0 "env + id of another kind passes with a note" \
    guard "$fx_id" env workspace_fixture linux rust
expect_rc 0 "env + right kind, wrong coordinates passes with a note" \
    guard "$ws_id" env workspace_fixture linux cpp

# Right kind, wrong coordinates, aimed deliberately: still a wrong invocation.
expect_rc 2 "flag + right kind, wrong coordinates is fatal" \
    guard "$ws_id" flag workspace_fixture linux cpp

echo "check-fixture-id-guard: platform vocabulary"
expect_rc 2 "unknown platform is fatal" \
    bash -c 'source scripts/build/fixture-id-guard.sh; nros_fixture_require_known_platform natve'

echo "check-fixture-id-guard: wired into the builders"
# End to end through a real builder — proves the guard is actually reached,
# not merely present. fixtures-build.sh needs no CLI or SDK to get this far.
expect_rc 2 "fixtures-build.sh --id of a workspace row is fatal" \
    bash scripts/build/fixtures-build.sh linux rust --id "$ws_id"

# And the case that must stay silent and green: no id filter, a real platform,
# a language with no rows. Sweeps hit this constantly.
empty_out="$(bash scripts/build/fixtures-build.sh threadx-linux mixed 2>&1)"
empty_rc=$?
if [ "$empty_rc" -ne 0 ]; then
    echo "  FAIL  unfiltered empty coordinate: expected rc=0, got rc=${empty_rc}"
    fails=$((fails + 1))
elif [ -n "$empty_out" ]; then
    echo "  FAIL  unfiltered empty coordinate: expected silence, got: ${empty_out}"
    fails=$((fails + 1))
else
    echo "  ok    unfiltered empty coordinate stays silent and green"
fi

if [ "$fails" -ne 0 ]; then
    echo "check-fixture-id-guard: ${fails} case(s) failed" >&2
    exit 1
fi
echo "check-fixture-id-guard: OK"
